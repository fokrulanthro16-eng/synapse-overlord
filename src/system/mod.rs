use sysinfo::System;

pub struct SystemSnapshot {
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
}

pub struct SystemMonitor {
    sys: System,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self { sys }
    }

    pub fn snapshot(&mut self) -> SystemSnapshot {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();

        let cpu_percent = self.sys.global_cpu_usage();
        let ram_used = self.sys.used_memory();
        let ram_total = self.sys.total_memory();

        let ram_percent = if ram_total > 0 {
            (ram_used as f32 / ram_total as f32) * 100.0
        } else {
            0.0
        };

        SystemSnapshot {
            cpu_percent,
            ram_percent,
            ram_used_mb: ram_used / 1024 / 1024,
            ram_total_mb: ram_total / 1024 / 1024,
        }
    }
}

impl SystemSnapshot {
    #[allow(dead_code)]
    pub fn ram_warning(&self) -> bool {
        self.ram_percent >= 75.0
    }

    pub fn ram_critical(&self) -> bool {
        self.ram_percent >= 90.0
    }
}
