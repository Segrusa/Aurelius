//! Sortable UUIDv7 identifiers.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            Serialize,
            Deserialize,
            schemars::JsonSchema,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn parse(text: &str) -> Result<Self, uuid::Error> {
                Uuid::parse_str(text).map(Self)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(ProjectId);
id_type!(ScopeId);
id_type!(EventId);
id_type!(CommandId);
id_type!(JobId);
id_type!(BlobId);
id_type!(HostId);
id_type!(ClientId);
id_type!(PrincipalId);
id_type!(ConversationId);
id_type!(MessageId);
id_type!(RunId);
id_type!(EmployeeId);
id_type!(ApprovalId);
id_type!(ProviderId);
id_type!(TaskId);
id_type!(AttachmentId);
id_type!(GrantId);
id_type!(MemoryId);
id_type!(ScheduledMessageId);
id_type!(IdeaId);
id_type!(TaskScheduleId);
id_type!(ProposalId);
id_type!(TicketId);
id_type!(BoardColumnId);
id_type!(UserId);
id_type!(SessionId);
id_type!(VaultItemId);
id_type!(GuidelineId);
id_type!(GuidelineProposalId);
