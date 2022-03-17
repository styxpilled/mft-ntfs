mod err;
mod mft;
mod privileges;
mod volumes;

#[cfg(feature = "progress")]
use indicatif::{HumanDuration, ProgressBar, ProgressIterator};
use winapi::um::{handleapi::CloseHandle, winnt::HANDLE};

use std::{collections::HashMap, convert::TryInto as _, ffi::OsString, ops::Deref};

#[derive(Debug)]
pub struct SafeHandle {
  handle: HANDLE,
}
impl Deref for SafeHandle {
  type Target = HANDLE;

  fn deref(&self) -> &Self::Target {
    &self.handle
  }
}
impl Drop for SafeHandle {
  fn drop(&mut self) {
    unsafe { CloseHandle(self.handle) };
  }
}

pub struct Entry {
  pub name: OsString,
  pub path: String,
  pub real_size: u64,
  pub alloc_size: u64,
  pub is_dir: bool,
}

pub struct Filesystem {
  pub drive_letter: String,
  pub entries: HashMap<u64, mft::MftEntry>,
  pub entrylist: Vec<u64>,
  pub files: HashMap<String, Entry>,
  bytes_per_cluster: u64,
}

impl Filesystem {
  fn new(drive_letter: OsString, bytes_per_cluster: u64, entry_count: usize) -> Self {
    Filesystem {
      drive_letter: {
        let mut drive_letter = drive_letter.to_string_lossy().into_owned();
        if drive_letter.ends_with('\\') {
          drive_letter.pop();
        }
        drive_letter
      },
      entries: HashMap::with_capacity(entry_count),
      entrylist: Vec::with_capacity(entry_count),
      files: HashMap::with_capacity(entry_count),
      bytes_per_cluster,
    }
  }
  fn get_full_path(&self, entry: u64) -> Option<String> {
    let mut entry = self.entries.get(&entry)?;
    let mut result = self.drive_letter.clone();

    let mut parts = Vec::new();

    loop {
      let parents = entry.parents();

      if parents.len() == 0 {
        break;
      }
      if parents[0] != entry.base_record_segment_idx {
        parts.push(entry.get_best_filename()?);
        entry = self.entries.get(&parents[0])?;
      } else {
        break;
      }
    }

    for part in parts.iter().rev() {
      result.push('\\');
      result.push_str(&part.to_string_lossy());
    }

    Some(result)
  }

  fn add_entry(&mut self, entry: mft::MftEntry) {
    self.entrylist.push(entry.base_record_segment_idx);
    self.entries.insert(entry.base_record_segment_idx, entry);
  }

  fn add_fs_entry(&mut self, entry: &u64) {
    let mut entry = self.entries.get(&entry).unwrap();
    let mut real_size = 0;
    let alloc_size = entry.get_allocated_size(self.bytes_per_cluster);
    let id = entry.base_record_segment_idx;
    let name = entry.get_best_filename().unwrap_or(OsString::from(""));
    let parents = entry.parents();

    for i in 0..entry.data.len() {
      real_size += entry.data[i].logical_size;
    }

    let path = self.get_full_path(id).unwrap();

    self
      .files
      .entry(path.clone())
      .and_modify(|file| {
        file.real_size = file.real_size;
        file.path = path.clone();
        file.name = name.clone();
        file.is_dir = file.is_dir;
      })
      .or_insert(Entry {
        name: name.clone(),
        path: path.clone(),
        alloc_size,
        real_size,
        is_dir: false,
      });

    loop {
      let parents = entry.parents();

      if parents.len() == 0 {
        break;
      } else if parents[0] != entry.base_record_segment_idx {
        let name = self
          .entries
          .get(&parents[0])
          .unwrap()
          .get_best_filename()
          .unwrap();
        let path = self.get_full_path(parents[0]).unwrap();
        self
          .files
          .entry(path.clone())
          .and_modify(|file| {
            file.real_size += real_size;
            file.is_dir = true;
          })
          .or_insert(Entry {
            name,
            path,
            real_size,
            alloc_size,
            is_dir: true,
          });
        entry = &*self.entries.get(&parents[0]).unwrap();
      } else {
        break;
      }
    }

    if parents.len() != 0 {
      let name = self
        .entries
        .get(&parents[0])
        .unwrap()
        .get_best_filename()
        .unwrap();
      let path = self.get_full_path(parents[0]).unwrap();
      self
        .files
        .entry(path.clone())
        .and_modify(|file| {
          file.real_size += real_size;
          file.is_dir = true;
        })
        .or_insert(Entry {
          name,
          path,
          real_size,
          alloc_size,
          is_dir: true,
        });
    }
  }
}

fn handle_volume(volume: volumes::VolumeInfo) -> Result<Filesystem, err::Error> {
  #[cfg(feature = "progress")]
  println!("Reading {}...", volume.paths[0].to_string_lossy());

  #[cfg(feature = "progress")]
  let begin = std::time::Instant::now();
  
  let handle = volume.get_handle()?;
  let (mft, bytes_per_cluster) = mft::MasterFileTable::load(handle, &volume.paths[0])?;
  
  #[cfg(feature = "progress")]
  let entry_count = mft.entry_count();

  let mut filesystem = Filesystem::new(
    volume.paths[0].clone(),
    bytes_per_cluster,
    mft.entry_count().try_into().unwrap(),
  );

  #[cfg(feature = "progress")]
  let progress = ProgressBar::new(entry_count);
  #[cfg(feature = "progress")]
  progress.set_draw_delta(entry_count / 20);
  for entry in mft {
    filesystem.add_entry(entry?);
    #[cfg(feature = "progress")]
    progress.inc(1);
  }

  #[cfg(feature = "progress")]
  println!("Creating queryable filesystem");
  #[cfg(feature = "progress")]
  let progress = ProgressBar::new(entry_count);
  #[cfg(feature = "progress")]
  progress.set_draw_delta(entry_count / 20);
  for entry in filesystem.entrylist.clone() {
    filesystem.add_fs_entry(&entry);
    #[cfg(feature = "progress")]
    progress.inc(1);
  }

  #[cfg(feature = "progress")]
  let time_taken = begin.elapsed();
  #[cfg(feature = "progress")]
  println!(
    "Read {} MFT entries in {} ({:.0} entries/sec)",
    entry_count,
    HumanDuration(time_taken),
    1000f64 * (entry_count as f64) / (time_taken.as_millis() as f64)
  );
  Ok(filesystem)
}

pub fn main(drive_letters: Option<Vec<char>>) -> Result<Vec<Filesystem>, err::Error> {
  match privileges::has_sufficient_privileges() {
    Ok(true) => {}
    Ok(false) => {
      eprintln!("This program must be run elevated!");
    }
    Err(err) => {
      eprintln!("Failed to check privilege level: {:?}", err);
      println!("Continuing anyway, although things will probably fail.");
    }
  }

  let mut result = Vec::new();
  for volume in volumes::VolumeIterator::new().unwrap() {
    match volume {
      Ok(volume) => {
        if !volume.paths.is_empty() {
          if let Some(ref whitelist) = drive_letters {
            let checker = |path: &OsString| {
              if let Some(first_char) = path.to_string_lossy().chars().next() {
                whitelist.contains(&first_char)
              } else {
                false
              }
            };

            if !volume.paths.iter().any(checker) {
              continue;
            }
          }

          match handle_volume(volume) {
            Ok(volume) => {
              result.push(volume);
            }
            Err(err) => {
              eprintln!("Failed to process volume: {:?}", err);
            }
          }
        }
      }
      Err(err) => {
        eprintln!("VolumeIterator produced an error: {:?}", err);
      }
    }
  }
  Ok(result)
}
