use anyhow::Context;

use crate::config::LoggingOptions;

pub fn init_logging(config: &LoggingOptions) -> anyhow::Result<()> {
    if config.journald.get() {
        init_systemd_journal_logger(config)
    } else {
        init_env_logger(config)
    }
}

fn init_env_logger(config: &LoggingOptions) -> anyhow::Result<()> {
    let mut log_builder = env_logger::builder();
    if config.disable_timestamps.get() {
        log_builder.format_timestamp(None);
    }
    if let Some(log_level) = config.log_level {
        log_builder.filter_level(log_level);
    }
    log_builder.init();
    Ok(())
}

fn init_systemd_journal_logger(config: &LoggingOptions) -> anyhow::Result<()> {
    systemd_journal_logger::JournalLog::new()
        .context("Failed to crate journal log")?
        .install()
        .context("Failed to install journal log")?;
    if let Some(log_level) = config.log_level {
        log::set_max_level(log_level);
    }
    Ok(())
}
