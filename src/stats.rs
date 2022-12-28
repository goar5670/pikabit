use std::{
    time::SystemTime,
    io::{stdout, Write},
};

enum SpeedUnit {
    Bytes,
    KBs,
    MBs,
}

impl SpeedUnit {
    fn format(&self) -> &'static str {
        match &self {
            Self::Bytes => "Bytes/s",
            Self::KBs => "KB/s",
            Self::MBs => "MB/s",
        }
    }
}

pub struct Speed {
    value: f64,
    unit: SpeedUnit,
}

impl Speed {
    fn new(d: f64) -> Self {
        let (value, unit) = match d {
            bytes if bytes < 512.0 => (bytes, SpeedUnit::Bytes),
            kbs if kbs < 512.0 * 1024.0 => (kbs / 1024.0, SpeedUnit::KBs),
            mbs => (mbs / (1024.0 * 1024.0), SpeedUnit::MBs),
        };

        Self { value, unit }
    }

    fn format(&self) -> String {
        format!("{:.2} {}   ", self.value, self.unit.format())
    }
}

pub struct StatsTracker {
    downloaded: u64,
    latent: u64,
    total: u64,
    last_printed: SystemTime,
}

impl StatsTracker {
    pub fn new(downloaded: Option<u64>, total: u64) -> Self {
        Self {
            downloaded: downloaded.unwrap_or(0),
            latent: 0,
            total,
            last_printed: SystemTime::now(),
        }
    }

    fn format_progress(progress: u8) -> String {
        const SCALE: usize = 50;
        debug_assert!(progress <= 100);
        let scaled = (progress as f64 / 100.0 * (SCALE as f64)) as usize;
        let bar = match scaled {
            SCALE => "[".to_owned() + &"=".repeat(SCALE) + "] 100%",
            p => {
                "[".to_owned()
                    + &"=".repeat(p)
                    + ">"
                    + &" ".repeat(SCALE - p - 1)
                    + "] "
                    + &progress.to_string()
                    + "%"
            }
        };

        bar
    }

    pub fn update(&mut self, piece_length: u32) {
        self.latent += piece_length as u64;
    }

    pub fn print(&mut self) {
        let mut stdout = stdout();
        let speed =
            Speed::new((self.latent as f64) / (self.last_printed.elapsed().unwrap().as_secs_f64()));

        self.downloaded += self.latent;
        self.latent = 0;
        self.last_printed = SystemTime::now();

        debug_assert!(
            self.downloaded <= self.total,
            "downloaded is more than total, {}, {}",
            self.downloaded,
            self.total
        );
        let progress = ((self.downloaded as f64 / self.total as f64) * 100.0).round() as u8;

        print!("\r{} {}", Self::format_progress(progress), speed.format());
        stdout.flush().unwrap();
    }
}
