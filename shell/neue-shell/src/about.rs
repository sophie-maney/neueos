//! The About This Computer panel (task 3.2b): the system's name and version, the kernel,
//! the processor and the memory, read from `/etc/os-release` and `/proc`. Parsing is
//! portable and tested on fixtures; reading happens on Linux.

use neue_kit::{Align, Element, Label, Stack, TextStyle, Window};

/// What the panel shows. Missing facts are empty and left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct About {
    /// `PRETTY_NAME` from os-release, else `NAME`.
    pub system: String,
    /// `VERSION` or `BUILD_ID` from os-release.
    pub version: String,
    /// The kernel release.
    pub kernel: String,
    /// The first processor's model name.
    pub processor: String,
    /// Total memory, in GB with one decimal (`7.7 GB`).
    pub memory: String,
}

impl About {
    /// Reads the facts of this machine.
    pub fn read() -> Self {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let os = read("/etc/os-release");
        let os = if os.is_empty() {
            read("/usr/lib/os-release")
        } else {
            os
        };
        Self::parse(
            &os,
            &read("/proc/sys/kernel/osrelease"),
            &read("/proc/cpuinfo"),
            &read("/proc/meminfo"),
        )
    }

    /// The facts from the contents of os-release, the kernel release, cpuinfo and meminfo.
    pub fn parse(os_release: &str, kernel: &str, cpuinfo: &str, meminfo: &str) -> Self {
        let os = |key: &str| {
            os_release.lines().find_map(|line| {
                let value = line.strip_prefix(key)?.strip_prefix('=')?;
                Some(value.trim().trim_matches('"').to_owned())
            })
        };
        let system = os("PRETTY_NAME").or_else(|| os("NAME")).unwrap_or_default();
        let version = os("VERSION").or_else(|| os("BUILD_ID")).unwrap_or_default();
        let field = |text: &str, key: &str| {
            text.lines().find_map(|line| {
                let (k, v) = line.split_once(':')?;
                (k.trim() == key).then(|| v.trim().to_owned())
            })
        };
        // x86 says "model name"; arm64 kernels often only give "CPU implementer" etc.
        let processor = field(cpuinfo, "model name")
            .or_else(|| field(cpuinfo, "Model"))
            .unwrap_or_default();
        let memory = field(meminfo, "MemTotal")
            .and_then(|v| v.trim_end_matches("kB").trim().parse::<u64>().ok())
            .map(|kib| {
                #[expect(clippy::cast_precision_loss, reason = "a size for display")]
                let gb = kib as f64 / (1024.0 * 1024.0);
                format!("{gb:.1} GB")
            })
            .unwrap_or_default();
        Self {
            system,
            version,
            kernel: kernel.trim().to_owned(),
            processor,
            memory,
        }
    }

    /// The panel's content.
    pub fn content<M: 'static>(&self) -> Element<M> {
        let title = if self.system.is_empty() {
            "neueOS"
        } else {
            &self.system
        };
        let mut column = Stack::column()
            .padding(20.0)
            .align(Align::Center)
            .with(Label::new(title).style(TextStyle::Title));
        if !self.version.is_empty() {
            column = column.with(Label::new(format!("Version {}", self.version)).secondary());
        }
        for (name, value) in [
            ("Processor", &self.processor),
            ("Memory", &self.memory),
            ("Kernel", &self.kernel),
        ] {
            if !value.is_empty() {
                column = column.with(Label::new(format!("{name}: {value}")));
            }
        }
        column.into()
    }

    /// The panel's window.
    pub fn window<M: 'static>(&self) -> Window<M> {
        Window::new("About This Computer", (360, 200), self.content())
            .app_id("org.neueos.Shell.About")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_facts() {
        let about = About::parse(
            "NAME=\"Arch Linux\"\nPRETTY_NAME=\"neueOS\"\nBUILD_ID=rolling\n",
            "6.17.7-arch1-1\n",
            "processor\t: 0\nmodel name\t: AMD Ryzen 7 7840U\n",
            "MemTotal:        8040452 kB\nMemFree: 1 kB\n",
        );
        assert_eq!(about.system, "neueOS");
        assert_eq!(about.version, "rolling");
        assert_eq!(about.kernel, "6.17.7-arch1-1");
        assert_eq!(about.processor, "AMD Ryzen 7 7840U");
        assert_eq!(about.memory, "7.7 GB");
    }

    #[test]
    fn missing_facts_are_empty() {
        assert_eq!(About::parse("", "", "", ""), About::default());
    }
}
