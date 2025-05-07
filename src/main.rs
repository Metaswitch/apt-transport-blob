// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
use bytes::BufMut;

use log::{debug, error, info, LevelFilter, Record};
use log4rs::filter::{Filter, Response};
use message::{Message, MessageType};

use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;

mod azure;
mod azure_credential_interop;
mod message;
mod processor;

// Hard-coded function to send the capabilities of this transport
fn send_capabilities() {
    let version = env!("CARGO_PKG_VERSION");
    Message::new(
        MessageType::Capabilities,
        vec![
            ("Version", version),
            ("Send-Config", "true"),
            ("Single-Instance", "true"),
        ],
    )
    .send()
}

// LCOV_EXCL_START

#[derive(Debug)]
pub struct AzureTransportFilter {}
impl Filter for AzureTransportFilter {
    fn filter(&self, record: &Record) -> Response {
        match record.module_path() {
            Some(module) => {
                if module.starts_with("azure_core::policies::transport") {
                    Response::Reject
                } else {
                    Response::Neutral
                }
            }
            None => Response::Neutral,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let azure_filter = Box::new(AzureTransportFilter {});

    // Set up the logger to log to the /var/log directory
    let appender = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d} [{l}] <{M}:{L}> {m}{n}")))
        .build("/var/log/apt-transport-blob.log")?;

    let config = Config::builder()
        .appender(
            Appender::builder()
                // Ensure secure logs aren't logged out
                .filter(azure_filter)
                .build("default", Box::new(appender)),
        )
        .build(
            Root::builder()
                .appender("default")
                .build(LevelFilter::Debug),
        )?;

    let _handle = log4rs::init_config(config)?;

    // Set up a message Processor
    let processor = processor::Processor::new()?;

    let mut input_buffer = vec![];

    // Print our capabilities
    send_capabilities();

    info!("Ready to receive messages");

    // Read the input on a loop until there's a double newline
    loop {
        let mut buffer = String::new();
        let bytes = std::io::stdin().read_line(&mut buffer)?;
        if bytes == 0 {
            debug!("EOF reached");
            break;
        }

        debug!("Buffer: {:?}", buffer);
        // Write the buffer to our message buffer
        input_buffer.put(buffer.as_bytes());

        if buffer == "\n" {
            info!("Empty line reached, process message");
            // Parse the message
            match message::Message::from_bytes(&input_buffer) {
                Ok(msg) => {
                    // Process the message
                    match processor.process(msg).await {
                        Ok(_) => {
                            // Log the success
                            info!("Message processed successfully");
                        }
                        Err(err) => {
                            // This is an unexpected error; log a general
                            // failure then exit.
                            error!("Error: {:?}", err);
                            Message::send_general_failure(&format!("Error: {}", err));
                            return Err(err);
                        }
                    }
                }
                Err(err) => {
                    // Log the error
                    info!("Error: {:?}", err);
                }
            }

            // Clear the message buffer
            input_buffer.clear();
        }
    }

    Ok(())
}

// LCOV_EXCL_STOP

#[cfg(test)]
mod tests {
    use super::*;
    use env_logger::Env;

    pub fn init_logger() {
        let _ = env_logger::Builder::from_env(Env::default().default_filter_or("trace"))
            .is_test(true)
            .try_init();
    }

    pub fn cover_debug(thing: &impl std::fmt::Debug) {
        let _ = format!("{:?}", thing);
    }

    pub fn cover_error(error: &impl std::error::Error) {
        let _ = error.source();
    }

    #[test]
    fn test_send_capabilities() {
        send_capabilities()
    }
}
