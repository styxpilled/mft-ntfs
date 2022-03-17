# mft-ntfs

A Rust library for parsing the Windows NTFS MFT that provides an API for querying the MFT.

`mft-ntfs` is based on [koakuma](https://github.com/zrneely/koakuma) by [Zachary Neely](https://github.com/zrneely). Huge thanks to them, as this project would not be possible without them, as figuring out all of the Windows syscalls and magical MFT bytes is beyond my abilities. Also apologies to absolutely butchering all of their existing code.

Currently the library has quite poor performance for my taste (12-14 seconds to create a queryable filesystem on my 465GB HDD), due to the way I implemented creating the filesystem. I can't think of anything better for now, but I'm sure there is a better way of doing this.

`mft-ntfs` currently has an optional `progress` feature, which shows a little loading bar while it's reading the MFT and creating the queryable fs.

## INSTALLATION

Add this your `Cargo.toml`:

```toml
[dependencies]
mft_ntfs = { git = "https://github.com/styxpilled/mft-ntfs" }
```

If you want to use the progress feature:

```toml
[dependencies]
mft_ntfs = { git = "https://github.com/styxpilled/mft-ntfs", features = ["progress"] }
```
