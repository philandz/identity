pub mod pb {
    pub mod common {
        pub mod base {
            tonic::include_proto!("common.base");
        }
    }
    pub mod service {
        pub mod identity {
            tonic::include_proto!("service.identity");
        }
    }
    pub mod shared {
        pub mod user {
            tonic::include_proto!("shared.user");
        }
        pub mod organization {
            tonic::include_proto!("shared.organization");
        }
    }
}

pub mod converters;
pub mod handler;
pub mod manager;
