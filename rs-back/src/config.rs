#[derive(Debug, Clone)]
pub struct Config {
    pub db: String,
    pub base: String,
    pub master: String,
    pub http_port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        fn var(key: &str, default: &str) -> String {
            std::env::var(key).unwrap_or_else(|_| default.to_string())
        }
        Config {
            db: var("DB", "postgresql://swift:swift@localhost:5432/swift"),
            base: var("BASE", "http://back.bwrrc.org.cn"),
            master: var("MASTER", "0.0.0.0:12345"),
            http_port: var("HTTP_PORT", "20000").parse().unwrap_or(20000),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_unset() {
        // Clear env to assert defaults (single-threaded test).
        std::env::remove_var("DB");
        std::env::remove_var("BASE");
        std::env::remove_var("MASTER");
        std::env::remove_var("HTTP_PORT");
        let c = Config::from_env();
        assert_eq!(c.db, "postgresql://swift:swift@localhost:5432/swift");
        assert_eq!(c.base, "http://back.bwrrc.org.cn");
        assert_eq!(c.master, "0.0.0.0:12345");
        assert_eq!(c.http_port, 20000);
    }
}
