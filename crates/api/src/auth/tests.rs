#[cfg(test)]
mod unit_tests {
    use uuid::Uuid;

    use crate::{
        auth::{
            claims::{decode_access_token, encode_access_token, Claims, ACCESS_TOKEN_TTL_SECS},
            password::{hash_password, verify_password},
            service::validate_password,
        },
        models::workspace::{MemberRole, WorkspaceKind},
    };
    use chrono::Utc;
    use sha2::{Digest, Sha256};

    #[test]
    fn test_password_hash_verify() {
        let hash = hash_password("correct-horse-battery").unwrap();
        assert!(verify_password("correct-horse-battery", &hash).is_ok());
    }

    #[test]
    fn test_password_verify_wrong_fails() {
        let hash = hash_password("correct-horse-battery").unwrap();
        assert!(verify_password("wrong-password", &hash).is_err());
    }

    #[test]
    fn test_jwt_roundtrip() {
        let secret = b"test-secret-at-least-32-bytes-long!!";
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let now = Utc::now();

        let claims = Claims {
            sub: user_id,
            email: "test@example.com".into(),
            workspace_id,
            workspace_kind: WorkspaceKind::Collector,
            role: MemberRole::Owner,
            iat: now.timestamp(),
            exp: now.timestamp() + ACCESS_TOKEN_TTL_SECS,
        };

        let token = encode_access_token(&claims, secret).unwrap();
        let decoded = decode_access_token(&token, secret).unwrap();

        assert_eq!(decoded.sub, user_id);
        assert_eq!(decoded.email, "test@example.com");
        assert_eq!(decoded.workspace_id, workspace_id);
        assert!(matches!(decoded.workspace_kind, WorkspaceKind::Collector));
        assert!(matches!(decoded.role, MemberRole::Owner));
    }

    #[test]
    fn test_jwt_wrong_secret_fails() {
        let secret = b"test-secret-at-least-32-bytes-long!!";
        let wrong  = b"wrong-secret-at-least-32-bytes-lon!";
        let now = Utc::now();
        let claims = Claims {
            sub: Uuid::new_v4(),
            email: "x@x.com".into(),
            workspace_id: Uuid::new_v4(),
            workspace_kind: WorkspaceKind::Seller,
            role: MemberRole::Staff,
            iat: now.timestamp(),
            exp: now.timestamp() + 900,
        };
        let token = encode_access_token(&claims, secret).unwrap();
        assert!(decode_access_token(&token, wrong).is_err());
    }

    #[test]
    fn test_token_hash_deterministic() {
        let raw = "some-random-token-value";
        let h1 = format!("{:x}", Sha256::digest(raw.as_bytes()));
        let h2 = format!("{:x}", Sha256::digest(raw.as_bytes()));
        assert_eq!(h1, h2);
        assert_ne!(h1, raw);
    }

    #[test]
    fn test_validate_password_too_short() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("exactly8").is_ok());
        assert!(validate_password("longer-password").is_ok());
    }
}
