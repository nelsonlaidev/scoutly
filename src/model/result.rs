use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResultKind {
    Response,
    Blocked,
    Failed,
    Skipped,
    Invalid,
}

impl ResultKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Response => "response",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Invalid => "invalid",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureReason {
    RequestTimedOut,
    ConnectionFailed,
    RequestFailed,
    AntiBotChallenge,
    UnsupportedProtocol,
    InvalidUrl,
}

impl FailureReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestTimedOut => "request-timed-out",
            Self::ConnectionFailed => "connection-failed",
            Self::RequestFailed => "request-failed",
            Self::AntiBotChallenge => "anti-bot-challenge",
            Self::UnsupportedProtocol => "unsupported-protocol",
            Self::InvalidUrl => "invalid-url",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FailureReason, ResultKind};

    #[test]
    fn display_names_match_serialized_enum_values() {
        for (kind, expected) in [
            (ResultKind::Response, "response"),
            (ResultKind::Blocked, "blocked"),
            (ResultKind::Failed, "failed"),
            (ResultKind::Skipped, "skipped"),
            (ResultKind::Invalid, "invalid"),
        ] {
            assert_eq!(kind.as_str(), expected);
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{expected}\"")
            );
        }

        for reason in [
            FailureReason::RequestTimedOut,
            FailureReason::ConnectionFailed,
            FailureReason::RequestFailed,
            FailureReason::AntiBotChallenge,
            FailureReason::UnsupportedProtocol,
            FailureReason::InvalidUrl,
        ] {
            assert_eq!(
                serde_json::to_string(&reason).unwrap(),
                format!("\"{}\"", reason.as_str())
            );
        }
    }
}
