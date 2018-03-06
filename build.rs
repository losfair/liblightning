extern crate cc;

trait SetupForPlatform {
    fn setup_for_platform(&mut self, platform: &str) -> &mut Self;
}

impl SetupForPlatform for cc::Build {
    fn setup_for_platform(&mut self, platform: &str) -> &mut Self {
        match platform {
            "x86_64-unknown-linux-gnu"
            | "x86_64-unknown-linux-musl" => {
                self.file("platforms/x86_64-linux/sched_helper.s")
            },
            _ => panic!("Unsupported platform: {}", platform)
        }
    }
}

fn main() {
    cc::Build::new()
        .setup_for_platform(std::env::var("TARGET").unwrap_or_else(|e| {
            panic!("Error while reading the TARGET environment variable: {:?}", e);
        }).as_str())
        .compile("lightning_platform");
}
