// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
use log::{debug, error, info, warn};
use url::Url;

use crate::{
    azure::AzureRegistry,
    message::{Message, MessageType},
};

macro_rules! unwrap_or_urifail {
    ($uri: expr, $result:expr) => {
        match $result {
            Ok(value) => value,
            Err(err) => {
                let message = format!("Error: {}", err);
                error!("URI failure for {}: {}", $uri, message);
                return Ok(Message::build_uri_failure($uri, &message));
            }
        }
    };
}

pub struct Processor {
    azure_registry: AzureRegistry,
}

impl Processor {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Processor {
            azure_registry: AzureRegistry::new()?,
        })
    }

    pub async fn process(&self, message: Message) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Handling message: {}", message.description());
        match message.message_type {
            MessageType::Configuration => {
                info!("Configuration message received");
                // Currently, nothing is done with the configuration
            }
            MessageType::URIAcquire => {
                info!("URI Acquire message received");
                Message::send_status("Waiting for headers");

                // Try and acquire the URI.  A message will be returned on
                // success (or failure), which is then sent.
                self.uri_acquire(message).await?.send();
            }
            _ => {
                warn!("Unhandled message type: {}", message.description());
            }
        }
        Ok(())
    }

    pub async fn uri_acquire(
        &self,
        message: Message,
    ) -> Result<Message, Box<dyn std::error::Error>> {
        // Get the URI. It's part of the interface to have this field here,
        // so a missing URI is a terminal error.
        let uri = message.uri()?;
        info!("Acquiring URI: {uri}");

        // Get the filename to download to.
        let filename = unwrap_or_urifail!(uri, message.filename());
        info!("Filename: {filename}");

        // Parse the url.
        let url = unwrap_or_urifail!(uri, Url::parse(uri));
        info!("URL: {url}");

        let blob = unwrap_or_urifail!(uri, self.azure_registry.get_blob(&url));
        debug!("AzureBlob: {blob:?}");

        let blob_exists = unwrap_or_urifail!(uri, blob.exists().await);
        if !blob_exists {
            warn!("Blob doesn't exist! {uri}");
            let message = Message::build_uri_failure(uri, "Blob does not exist");
            return Ok(message);
        }

        // Get the blob's URI start fields.
        let (size, last_modified) = unwrap_or_urifail!(uri, blob.uri_start_fields().await);

        info!("Blob size: {size}");
        info!("Last modified: {last_modified}");

        // Send a URI Start to indicate we're starting the transfer.
        Message::send_uri_start(uri, size, &last_modified);
        info!("Sent URI start: {last_modified}");

        // Now actually download the URI
        let contents = unwrap_or_urifail!(uri, blob.download().await);

        info!("Downloaded blob: {uri}");
        // Write the contents to the file
        unwrap_or_urifail!(uri, std::fs::write(filename, contents));

        // Create a success response
        let message = Message::new(
            MessageType::URIDone,
            vec![("URI", uri), ("Filename", filename)],
        );
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::init_logger;

    #[tokio::test]
    async fn test_configuration() -> Result<(), Box<dyn std::error::Error>> {
        init_logger();
        let message = Message::new(MessageType::Configuration, vec![]);
        let processor = Processor::new()?;
        processor.process(message).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_unknown() -> Result<(), Box<dyn std::error::Error>> {
        init_logger();
        let message = Message::new(MessageType::Log, vec![]);
        let processor = Processor::new()?;
        processor.process(message).await?;
        Ok(())
    }
}
