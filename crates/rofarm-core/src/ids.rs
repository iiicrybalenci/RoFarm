use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self { Self(Uuid::new_v4()) }
            pub fn from_uuid(u: Uuid) -> Self { Self(u) }
            pub fn as_uuid(&self) -> Uuid { self.0 }
            pub fn is_nil(&self) -> bool { self.0.is_nil() }
        }

        impl Default for $name {
            fn default() -> Self { Self::new() }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(f) }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> { Uuid::parse_str(s).map(Self) }
        }
    };
}

id_type!(
    AccountId
);
id_type!(
    LaneId);
id_type!(
    ClientId
);
id_type!(
    LaunchProfileId
);
id_type!(
    SceneId
);
id_type!(
    AutomationId
);
id_type!(
    GameId
);
id_type!(
    ServerBookmarkId
);
id_type!(
    PrivateServerId
);
id_type!(
    SecretRecordId
);
id_type!(
    LayoutId
);
id_type!(
    WorkspaceId);
id_type!(
    PluginId
);
id_type!(
    DeviceId
);

pub mod builtin {
    use super::LaneId;
    use uuid::Uuid;

    fn fixed(s: &str) -> LaneId {
        let digest = blake3::hash(format!("rofarm.lane.{s}").as_bytes());
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        LaneId(Uuid::from_bytes(bytes))
    }

    pub fn farming() -> LaneId {
        fixed("farming")
    }
    pub fn idle() -> LaneId {
        fixed("idle")
    }
    pub fn trading() -> LaneId {
        fixed("trading")
    }
    pub fn testing() -> LaneId {
        fixed("testing")
    }
    pub fn all() -> [LaneId; 4] {
        [farming(), idle(), trading(), testing()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_lanes_are_stable_and_distinct() {
        assert_eq!(builtin::farming(), builtin::farming());
        assert_ne!(builtin::farming(), builtin::idle());
        let set: std::collections::BTreeSet<_> = builtin::all().into_iter().collect();
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn ids_round_trip_through_json_as_strings() {
        let id = AccountId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        let back: AccountId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
