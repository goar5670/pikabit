use crossterm::{cursor, terminal, ExecutableCommand};
use std::{
    io::{stdout, Stdout, Write},
    time::SystemTime,
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
        format!("{:.2} {}", self.value, self.unit.format())
    }
}

// todo: refine it to work with pieces instead of bytes
pub struct StatsTracker {
    downloaded: u64,
    latent: u64,
    total: u64,
    last_printed: SystemTime,
    piece_availability: f32,
    num_peers: u32,
    stdout: Stdout,
}

impl StatsTracker {
    pub fn new(downloaded: Option<u64>, total: u64) -> Self {
        Self {
            downloaded: downloaded.unwrap_or(0),
            latent: 0,
            total,
            last_printed: SystemTime::now(),
            piece_availability: 0.0,
            num_peers: 0,
            stdout: stdout(),
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
                    + &"-".repeat(SCALE - p - 1)
                    + "] "
                    + &progress.to_string()
                    + "%"
            }
        };

        bar
    }

    pub fn update_downloaded(&mut self, piece_length: u32) {
        self.latent += piece_length as u64;
    }

    pub fn update_availablity(&mut self, availability: f32) {
        self.piece_availability = availability;
    }

    pub fn update_peers(&mut self, peers: u32) {
        self.num_peers = peers;
    }

    pub fn print(&mut self) {
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
        let progress = ((self.downloaded as f64 / self.total as f64) * 100.0).floor() as u8;

        self.stdout
            .execute(terminal::Clear(terminal::ClearType::FromCursorDown))
            .unwrap();

        // todo: handle errors here
        writeln!(
            self.stdout,
            "\n{} {}",
            Self::format_progress(progress),
            speed.format()
        )
        .unwrap();
        writeln!(
            self.stdout,
            "Availability: {:.4}\t Peers: {}",
            self.piece_availability, self.num_peers
        )
        .unwrap();
        self.stdout.execute(cursor::MoveUp(3)).unwrap();
        self.stdout.flush().unwrap();
    }
}
