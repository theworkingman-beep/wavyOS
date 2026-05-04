//! FAT32 filesystem driver (read-only)
//!
//! Implements basic FAT32 directory traversal and file reading.
//! Supports long filenames via VFAT LFN entries.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::boxed::Box;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::fs::{self, DirEntry, FileHandle, FileSystem, FsError};

const BPB_BytsPerSec: usize = 0xB;
const BPB_SecPerClus: usize = 0xD;
const BPB_RsvdSecCnt: usize = 0xE;
const BPB_NumFATs: usize = 0x10;
const BPB_FATSz32: usize = 0x24;
const BPB_RootClus: usize = 0x2C;

const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_HIDDEN: u8 = 0x02;
const ATTR_SYSTEM: u8 = 0x04;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_LFN: u8 = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_VOLUME_ID;

static DISK_OFFSET: AtomicU64 = AtomicU64::new(0);

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct DirEntryRaw {
    name: [u8; 11],
    attr: u8,
    nt_res: u8,
    ctime_cs: u8,
    ctime: [u8; 2],
    cdate: [u8; 2],
    adate: [u8; 2],
    hi_clus: [u8; 2],
    mtime: [u8; 2],
    mdate: [u8; 2],
    lo_clus: [u8; 2],
    size: [u8; 4],
}

#[repr(C, packed)]
struct LfnEntry {
    seq: u8,
    name1: [u8; 10],
    attr: u8,
    type_: u8,
    chksum: u8,
    name2: [u8; 12],
    _zero: [u8; 2],
    name3: [u8; 4],
}

struct Fat32Fs {
    boot_sector: [u8; 512],
    bytes_per_sec: u16,
    sec_per_clus: u8,
    rsvd_sec_cnt: u16,
    num_fats: u8,
    fat_sz: u32,
    root_clus: u32,
    first_fat_sector: u32,
    first_data_sector: u32,
}

impl Fat32Fs {
    fn new(boot_sector: [u8; 512]) -> Self {
        let bytes_per_sec = u16::from_le_bytes([boot_sector[BPB_BytsPerSec], boot_sector[BPB_BytsPerSec + 1]]);
        let sec_per_clus = boot_sector[BPB_SecPerClus];
        let rsvd_sec_cnt = u16::from_le_bytes([boot_sector[BPB_RsvdSecCnt], boot_sector[BPB_RsvdSecCnt + 1]]);
        let num_fats = boot_sector[BPB_NumFATs];
        let fat_sz = u32::from_le_bytes([
            boot_sector[BPB_FATSz32],
            boot_sector[BPB_FATSz32 + 1],
            boot_sector[BPB_FATSz32 + 2],
            boot_sector[BPB_FATSz32 + 3],
        ]);
        let root_clus = u32::from_le_bytes([
            boot_sector[BPB_RootClus],
            boot_sector[BPB_RootClus + 1],
            boot_sector[BPB_RootClus + 2],
            boot_sector[BPB_RootClus + 3],
        ]);

        let first_fat_sector = rsvd_sec_cnt as u32;
        let first_data_sector = rsvd_sec_cnt as u32 + (num_fats as u32 * fat_sz);

        Self {
            boot_sector,
            bytes_per_sec,
            sec_per_clus,
            rsvd_sec_cnt,
            num_fats,
            fat_sz,
            root_clus,
            first_fat_sector,
            first_data_sector,
        }
    }

    fn read_sector(&self, sector: u32, buf: &mut [u8]) -> Result<(), FsError> {
        let offset = DISK_OFFSET.load(Ordering::Relaxed);
        let sector_offset = offset + (sector as u64 * self.bytes_per_sec as u64);

        unsafe {
            let src = (sector_offset as *const u8).add(0);
            let dst = buf.as_mut_ptr();
            core::ptr::copy_nonoverlapping(src, dst, self.bytes_per_sec as usize);
        }

        Ok(())
    }

    fn clus_to_sector(&self, cluster: u32) -> u32 {
        self.first_data_sector + (cluster - 2) * self.sec_per_clus as u32
    }

    fn get_next_cluster(&self, cluster: u32) -> Result<u32, FsError> {
        let fat_sector = self.first_fat_sector + (cluster * 4) / self.bytes_per_sec as u32;
        let mut fat_buf = [0u8; 512];
        self.read_sector(fat_sector, &mut fat_buf)?;

        let offset = (cluster * 4) % self.bytes_per_sec as u32;
        let next = u32::from_le_bytes([
            fat_buf[offset as usize],
            fat_buf[offset as usize + 1],
            fat_buf[offset as usize + 2],
            fat_buf[offset as usize + 3],
        ]) & 0x0FFFFFFF;

        Ok(next)
    }

    fn read_dir_entries(&self, cluster: u32) -> Result<Vec<(String, DirEntryRaw)>, FsError> {
        let mut entries = Vec::new();
        let mut current_clus = cluster;
        let mut lfn_name = String::new();
        let mut lfn_seq = 0u8;

        loop {
            let start_sector = self.clus_to_sector(current_clus);
            let mut buf = [0u8; 512];

            for sec_off in 0..self.sec_per_clus {
                self.read_sector(start_sector + sec_off as u32, &mut buf)?;

                let entry_size = core::mem::size_of::<DirEntryRaw>();
                for i in (0..self.bytes_per_sec as usize).step_by(entry_size) {
                    if i + entry_size > self.bytes_per_sec as usize {
                        break;
                    }

                    let raw_ptr = unsafe { buf.as_ptr().add(i) };
                    let first_byte = unsafe { *raw_ptr };

                    if first_byte == 0x00 {
                        return Ok(entries);
                    }
                    if first_byte == 0xE5 {
                        continue;
                    }

                    let raw = unsafe { &*(raw_ptr as *const DirEntryRaw) };

                    if raw.attr == ATTR_LFN {
                        let lfn = unsafe { &*(raw_ptr as *const LfnEntry) };
                        let seq = lfn.seq & 0x1F;

                        if seq == lfn_seq + 1 || lfn_seq == 0 {
                            lfn_seq = seq;
                            let mut name_chars = [0u16; 256];
                            let mut idx = 0;

                            for j in (0..10).step_by(2) {
                                if lfn.name1[j] == 0xFF { break; }
                                name_chars[idx] = u16::from_le_bytes([lfn.name1[j], lfn.name1[j + 1]]);
                                idx += 1;
                            }
                            for j in (0..12).step_by(2) {
                                if lfn.name2[j] == 0xFF { break; }
                                name_chars[idx] = u16::from_le_bytes([lfn.name2[j], lfn.name2[j + 1]]);
                                idx += 1;
                            }
                            for j in (0..4).step_by(2) {
                                if lfn.name3[j] == 0xFF { break; }
                                name_chars[idx] = u16::from_le_bytes([lfn.name3[j], lfn.name3[j + 1]]);
                                idx += 1;
                            }

                            let name = String::from_utf16_lossy(&name_chars[..idx]);
                            lfn_name = name + &lfn_name;
                        }

                        if lfn.seq & 0x40 != 0 {
                        }
                    } else if raw.attr & ATTR_VOLUME_ID == 0 {
                        let name = if !lfn_name.is_empty() {
                            let n = lfn_name.clone();
                            lfn_name.clear();
                            n
                        } else {
                            let mut name = String::new();
                            for &b in &raw.name {
                                if b == b' ' { break; }
                                name.push(b as char);
                            }
                            name.to_lowercase()
                        };

                        let size = u32::from_le_bytes(raw.size);
                        let file_type = if raw.attr & ATTR_DIRECTORY != 0 {
                            fs::FileType::Directory
                        } else {
                            fs::FileType::File
                        };

                        let _ = DirEntry {
                            name: name.clone(),
                            file_type,
                            size,
                        };

                        entries.push((name, *raw));
                    }
                }
            }

            let next = self.get_next_cluster(current_clus)?;
            if next >= 0x0FFFFFF8 {
                break;
            }
            current_clus = next;
        }

        Ok(entries)
    }

    fn find_path(&self, path: &str) -> Result<(DirEntryRaw, u32), FsError> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            let entries = self.read_dir_entries(self.root_clus)?;
            let (name, entry) = entries.first().ok_or(FsError::NotFound)?;
            let first_clus = u16::from_le_bytes(entry.lo_clus) as u32 | ((u16::from_le_bytes(entry.hi_clus) as u32) << 16);
            return Ok((*entry, first_clus));
        }

        let parts: Vec<&str> = path.split('/').collect();
        let mut current_clus = self.root_clus;

        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() { continue; }

            let entries = self.read_dir_entries(current_clus)?;
            let found = entries.iter().find(|(name, _)| name.eq_ignore_ascii_case(part));

            let (_, entry) = found.ok_or(FsError::NotFound)?;

            if i == parts.len() - 1 {
                let first_clus = u16::from_le_bytes(entry.lo_clus) as u32 | ((u16::from_le_bytes(entry.hi_clus) as u32) << 16);
                return Ok((*entry, first_clus));
            }

            if entry.attr & ATTR_DIRECTORY == 0 {
                return Err(FsError::NotADirectory);
            }

            current_clus = u16::from_le_bytes(entry.lo_clus) as u32 | ((u16::from_le_bytes(entry.hi_clus) as u32) << 16);
        }

        Err(FsError::NotFound)
    }

    fn read_file_clusters(&self, start_clus: u32, size: u32, buf: &mut [u8]) -> Result<usize, FsError> {
        let mut current_clus = start_clus;
        let mut bytes_read = 0;
        let total_to_read = size.min(buf.len() as u32) as usize;

        let clus_size = (self.bytes_per_sec as u32 * self.sec_per_clus as u32) as usize;
        let mut cluster_buf = alloc::vec![0u8; clus_size];

        loop {
            if bytes_read >= total_to_read {
                break;
            }

            let start_sector = self.clus_to_sector(current_clus);
            let mut offset_in_clus = 0;

            for sec_off in 0..self.sec_per_clus {
                self.read_sector(start_sector + sec_off as u32, &mut cluster_buf[offset_in_clus..offset_in_clus + self.bytes_per_sec as usize])?;
                offset_in_clus += self.bytes_per_sec as usize;
            }

            let remaining = total_to_read - bytes_read;
            let to_copy = remaining.min(clus_size);

            buf[bytes_read..bytes_read + to_copy].copy_from_slice(&cluster_buf[..to_copy]);
            bytes_read += to_copy;

            let next = self.get_next_cluster(current_clus)?;
            if next >= 0x0FFFFFF8 {
                break;
            }
            current_clus = next;
        }

        Ok(bytes_read)
    }
}

static FAT32_INSTANCE: spin::Once<Fat32Fs> = spin::Once::new();

pub fn init(disk_base: u64) {
    log::info!("fat32: initializing at {:x}", disk_base);

    DISK_OFFSET.store(disk_base, Ordering::Relaxed);

    let mut boot = [0u8; 512];
    unsafe {
        let src = disk_base as *const u8;
        core::ptr::copy_nonoverlapping(src, boot.as_mut_ptr(), 512);
    }

    let signature = u16::from_le_bytes([boot[0x1FE], boot[0x1FF]]);
    if signature != 0xAA55 {
        log::warn!("fat32: invalid boot signature: {:04X}", signature);
        return;
    }

    let fs = Fat32Fs::new(boot);
    log::info!("fat32: {} bytes/sec, {} sec/clus, root cluster {}",
        fs.bytes_per_sec, fs.sec_per_clus, fs.root_clus);

    FAT32_INSTANCE.call_once(|| fs);

    fs::register_fs(&*Box::leak(Box::new(Fat32FsWrapper)));
}

struct Fat32FsWrapper;

impl FileSystem for Fat32FsWrapper {
    fn open(&self, path: &str) -> Result<FileHandle, FsError> {
        let fs = FAT32_INSTANCE.get().ok_or(FsError::NotFound)?;
        let (entry, first_clus) = fs.find_path(path)?;
        let size = u32::from_le_bytes(entry.size);

        Ok(FileHandle {
            ino: first_clus,
            size,
            path: path.to_string(),
        })
    }

    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let fs = FAT32_INSTANCE.get().ok_or(FsError::NotFound)?;
        let (_entry, clus) = fs.find_path(path)?;

        let entries = fs.read_dir_entries(clus)?;
        Ok(entries.into_iter().map(|(name, entry)| {
            let size = u32::from_le_bytes(entry.size);
            let file_type = if entry.attr & ATTR_DIRECTORY != 0 {
                fs::FileType::Directory
            } else {
                fs::FileType::File
            };
            DirEntry { name, file_type, size }
        }).collect())
    }

    fn read(&self, handle: &FileHandle, offset: u32, buf: &mut [u8]) -> Result<usize, FsError> {
        let fs = FAT32_INSTANCE.get().ok_or(FsError::NotFound)?;

        if offset >= handle.size {
            return Ok(0);
        }

        let to_read = (handle.size - offset).min(buf.len() as u32) as usize;
        let mut temp_buf = alloc::vec![0u8; handle.size as usize];
        let bytes_read = fs.read_file_clusters(handle.ino, handle.size, &mut temp_buf)?;

        buf[..to_read].copy_from_slice(&temp_buf[offset as usize..offset as usize + to_read]);
        Ok(to_read.min(bytes_read))
    }
}
