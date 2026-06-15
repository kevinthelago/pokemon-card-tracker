#[cfg(test)]
mod unit_tests {
    use uuid::Uuid;

    use crate::{
        models::workspace::{MemberRole, WorkspaceKind},
        workspace::authz::AuthContext,
    };

    fn make_ctx(kind: WorkspaceKind, role: MemberRole) -> AuthContext {
        AuthContext {
            user_id: Uuid::new_v4(),
            email: "test@example.com".into(),
            workspace_id: Uuid::new_v4(),
            workspace_kind: kind,
            role,
        }
    }

    #[test]
    fn test_require_owner_staff_fails() {
        let ctx = make_ctx(WorkspaceKind::Seller, MemberRole::Staff);
        assert!(ctx.require_owner().is_err());
    }

    #[test]
    fn test_require_owner_owner_passes() {
        let ctx = make_ctx(WorkspaceKind::Seller, MemberRole::Owner);
        assert!(ctx.require_owner().is_ok());
    }

    #[test]
    fn test_require_seller_collector_fails() {
        let ctx = make_ctx(WorkspaceKind::Collector, MemberRole::Owner);
        assert!(ctx.require_seller().is_err());
    }

    #[test]
    fn test_require_seller_seller_passes() {
        let ctx = make_ctx(WorkspaceKind::Seller, MemberRole::Owner);
        assert!(ctx.require_seller().is_ok());
    }

    #[test]
    fn test_require_seller_owner_passes() {
        let ctx = make_ctx(WorkspaceKind::Seller, MemberRole::Owner);
        assert!(ctx.require_seller_owner().is_ok());
    }

    #[test]
    fn test_require_seller_owner_staff_fails() {
        let ctx = make_ctx(WorkspaceKind::Seller, MemberRole::Staff);
        assert!(ctx.require_seller_owner().is_err());
    }

    #[test]
    fn test_require_seller_owner_collector_owner_fails() {
        let ctx = make_ctx(WorkspaceKind::Collector, MemberRole::Owner);
        assert!(ctx.require_seller_owner().is_err());
    }
}
