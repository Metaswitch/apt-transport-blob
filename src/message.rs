// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
use std::fmt::Display;

use nom::bytes::complete::take_until;
use nom::character::complete::{char, digit1, newline, space0};
use nom::combinator::map_res;
use nom::multi::many0;
use nom::{IResult, Parser};

use log::error;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to parse message: {0}")]
    MessageParse(String),

    #[error("Too much message data")]
    MessageTooMuchData,

    #[error("Header not found: {0}")]
    HeaderNotFound(String),
}

#[derive(Debug, PartialEq)]
pub enum MessageType {
    Capabilities,
    Log,
    Status,
    URIStart,
    URIDone,
    URIFailure,
    GeneralFailure,
    URIAcquire,
    Configuration,
}

impl MessageType {
    fn code(&self) -> u16 {
        match self {
            MessageType::Capabilities => 100,
            MessageType::Log => 101,
            MessageType::Status => 102,
            MessageType::URIStart => 200,
            MessageType::URIDone => 201,
            MessageType::URIFailure => 400,
            MessageType::GeneralFailure => 401,
            MessageType::URIAcquire => 600,
            MessageType::Configuration => 601,
        }
    }

    fn description(&self) -> &str {
        match self {
            MessageType::Capabilities => "Capabilities",
            MessageType::Log => "Log",
            MessageType::Status => "Status",
            MessageType::URIStart => "URI Start",
            MessageType::URIDone => "URI Done",
            MessageType::URIFailure => "URI Failure",
            MessageType::GeneralFailure => "General Failure",
            MessageType::URIAcquire => "URI Acquire",
            MessageType::Configuration => "Configuration",
        }
    }

    pub fn from_bytes(input: &[u8]) -> IResult<&[u8], MessageType> {
        // The first line of a message is the message type and a description,
        // followed by a newline
        let (input, code) = digit1(input)?;
        let (input, _) = take_until("\n")(input)?;
        let (input, _) = newline(input)?;

        match code {
            b"100" => Ok((input, MessageType::Capabilities)),
            b"101" => Ok((input, MessageType::Log)),
            b"102" => Ok((input, MessageType::Status)),
            b"200" => Ok((input, MessageType::URIStart)),
            b"201" => Ok((input, MessageType::URIDone)),
            b"400" => Ok((input, MessageType::URIFailure)),
            b"401" => Ok((input, MessageType::GeneralFailure)),
            b"600" => Ok((input, MessageType::URIAcquire)),
            b"601" => Ok((input, MessageType::Configuration)),
            _ => unimplemented!("Unknown message type: {:?}", code),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Message {
    pub message_type: MessageType,
    pub headers: Vec<(String, String)>,
}

fn key_value_pair(input: &[u8]) -> IResult<&[u8], (String, String)> {
    let mut parse_key = map_res(take_until(":"), |buf| std::str::from_utf8(buf));
    let mut parse_value = map_res(take_until("\n"), |buf| std::str::from_utf8(buf));

    let (input, key) = parse_key.parse(input)?;
    let (input, _) = char(':')(input)?;
    let (input, _) = space0(input)?;
    let (input, value) = parse_value.parse(input)?;
    let (input, _) = newline(input)?;

    let res = (key.to_string(), value.to_string());
    Ok((input, res))
}

impl Message {
    //
    // Construction and logging functions cannot log, as they are used by the logger
    //
    pub fn new(message_type: MessageType, headers: Vec<(&str, &str)>) -> Message {
        Message {
            message_type,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn parse(input: &[u8]) -> IResult<&[u8], Message> {
        // Parse the MessageType from the message
        let (input, message_type) = MessageType::from_bytes(input)?;

        // Now take the headers; these are key-value pairs separated by a colon
        let (input, headers) = many0(key_value_pair).parse(input)?;

        // Now take the final newline.
        let (input, _) = newline(input)?;

        Ok((
            input,
            Message {
                message_type,
                headers,
            },
        ))
    }

    pub fn from_bytes(input: &[u8]) -> Result<Message, Error> {
        match Message::parse(input) {
            Ok((b"", message)) => Ok(message),
            Ok((_, _)) => Err(Error::MessageTooMuchData),
            Err(err) => Err(Error::MessageParse(format!("{}", err))),
        }
    }

    pub fn send(&self) {
        print!("{}", self);
    }

    pub fn send_status(message: &str) {
        Self::new(MessageType::Status, vec![("Message", message)]).send()
    }

    pub fn send_general_failure(message: &str) {
        Self::new(MessageType::GeneralFailure, vec![("Message", message)]).send()
    }

    pub fn send_uri_start(uri: &str, size: u64, last_modified: &str) {
        Self::new(
            MessageType::URIStart,
            vec![
                ("URI", uri),
                ("Size", &size.to_string()),
                ("Last-Modified", last_modified),
            ],
        )
        .send()
    }

    pub fn build_uri_failure(uri: &str, message: &str) -> Self {
        Self::new(
            MessageType::URIFailure,
            vec![("URI", uri), ("Message", message)],
        )
    }

    //
    // End of construction and logging functions
    //

    pub fn description(&self) -> String {
        format!(
            "{} {}",
            self.message_type.code(),
            self.message_type.description()
        )
    }

    fn header(&self, key: &str) -> Result<&str, Error> {
        self.headers
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .ok_or(Error::HeaderNotFound(key.to_string()))
    }

    pub fn uri(&self) -> Result<&str, Error> {
        self.header("URI")
    }

    pub fn filename(&self) -> Result<&str, Error> {
        self.header("Filename")
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} {}",
            self.message_type.code(),
            self.message_type.description()
        )?;
        for (key, value) in &self.headers {
            writeln!(f, "{}: {}", key, value)?;
        }
        writeln!(f)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{cover_debug, cover_error};

    fn check_parse(input: &[u8], expected: MessageType) {
        let (input, message) =
            MessageType::from_bytes(input).expect("Failed to parse message type");
        assert_eq!(message, expected);
        assert_eq!(input, &b""[..])
    }

    #[test]
    fn test_coverage() -> Result<(), Box<dyn std::error::Error>> {
        let message = Message::new(MessageType::Log, vec![]);
        cover_debug(&message);

        let error = Error::HeaderNotFound("text".to_string());
        cover_error(&error);
        cover_debug(&error);

        Ok(())
    }

    #[test]
    fn test_message_codes() {
        assert_eq!(MessageType::Capabilities.code(), 100);
        assert_eq!(MessageType::Log.code(), 101);
        assert_eq!(MessageType::Status.code(), 102);
        assert_eq!(MessageType::URIStart.code(), 200);
        assert_eq!(MessageType::URIDone.code(), 201);
        assert_eq!(MessageType::URIFailure.code(), 400);
        assert_eq!(MessageType::GeneralFailure.code(), 401);
        assert_eq!(MessageType::URIAcquire.code(), 600);
        assert_eq!(MessageType::Configuration.code(), 601);
    }

    #[test]
    fn test_message_descriptions() {
        assert_eq!(MessageType::Capabilities.description(), "Capabilities");
        assert_eq!(MessageType::Log.description(), "Log");
        assert_eq!(MessageType::Status.description(), "Status");
        assert_eq!(MessageType::URIStart.description(), "URI Start");
        assert_eq!(MessageType::URIDone.description(), "URI Done");
        assert_eq!(MessageType::URIFailure.description(), "URI Failure");
        assert_eq!(MessageType::GeneralFailure.description(), "General Failure");
        assert_eq!(MessageType::URIAcquire.description(), "URI Acquire");
        assert_eq!(MessageType::Configuration.description(), "Configuration");
    }

    #[test]
    fn test_message_type_from_bytes() {
        check_parse(b"100 Capabilities\n", MessageType::Capabilities);
        check_parse(b"101 Log\n", MessageType::Log);
        check_parse(b"102 Status\n", MessageType::Status);
        check_parse(b"200 URI Start\n", MessageType::URIStart);
        check_parse(b"201 URI Done\n", MessageType::URIDone);
        check_parse(b"400 URI Failure\n", MessageType::URIFailure);
        check_parse(b"401 General Failure\n", MessageType::GeneralFailure);
        check_parse(b"600 URI Acquire\n", MessageType::URIAcquire);
        check_parse(b"601 Configuration\n", MessageType::Configuration);
    }

    #[test]
    #[should_panic(expected = "Unknown message type")]
    fn test_unimplemented_message_type() {
        let _ = MessageType::from_bytes(b"999 Unknown\n").unwrap();
    }

    #[test]
    fn test_key_value_pair() {
        let (input, (key, value)) = key_value_pair(b"Key: Value\n").unwrap();
        assert_eq!(key, "Key");
        assert_eq!(value, "Value");
        assert_eq!(input, &b""[..]);
    }

    #[test]
    fn test_message_from_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let input = b"100 Capabilities\n\
                      Key: Value\n\
                      \n";
        let message = Message::from_bytes(input)?;
        assert_eq!(message.message_type, MessageType::Capabilities);

        let (key, value) = message.headers.first().unwrap();
        assert_eq!(key, "Key");
        assert_eq!(value, "Value");
        Ok(())
    }

    #[test]
    fn test_too_much_data() -> Result<(), Box<dyn std::error::Error>> {
        let input = b"100 Capabilities\n\
                      Key: Value\n\
                      \ntoo much data";
        let message = Message::from_bytes(input);
        match message {
            Err(Error::MessageTooMuchData) => (),
            _ => panic!("Unexpected error"), // LCOV_EXCL_LINE
        }
        Ok(())
    }

    #[test]
    fn test_buggy_message() -> Result<(), Box<dyn std::error::Error>> {
        let input = b"100 Capabilities\n\
                      No header line\n\
                      \n";
        let message = Message::from_bytes(input);
        match message {
            Err(Error::MessageParse(_)) => (),
            _ => panic!("Unexpected error"), // LCOV_EXCL_LINE
        }
        Ok(())
    }

    #[test]
    fn test_message_write() -> Result<(), Box<dyn std::error::Error>> {
        let message = Message {
            message_type: MessageType::Capabilities,
            headers: vec![("Key".to_string(), "Value".to_string())],
        };

        let output = format!("{}", message);
        assert_eq!(
            output,
            "100 Capabilities\n\
              Key: Value\n\
              \n"
        );
        Ok(())
    }

    #[test]
    fn test_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let message = Message {
            message_type: MessageType::Capabilities,
            headers: vec![("Key".to_string(), "Value".to_string())],
        };

        let output = format!("{}", message);
        let parsed_message = Message::from_bytes(output.as_bytes())?;
        assert_eq!(parsed_message, message);
        Ok(())
    }

    #[test]
    fn test_send_messages() -> Result<(), Box<dyn std::error::Error>> {
        Message::send_status("Hello, world");
        Message::send_general_failure("Goodbye, world");
        Message::send_uri_start("http://example.com", 123, "2021-01-01T00:00:00Z");
        let _ = Message::build_uri_failure("http://example.com", "Failed");
        Ok(())
    }

    #[test]
    fn test_description() {
        let message = Message {
            message_type: MessageType::Capabilities,
            headers: vec![],
        };
        assert_eq!(message.description(), "100 Capabilities");
    }
}
