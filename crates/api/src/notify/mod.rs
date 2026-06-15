use lettre::{message::header::ContentType, AsyncTransport, Message as EmailMessage};

use crate::{app::AppState, error::AppError, fraud::flags::RiskFlag};

/// Fan out in-app notifications and email to all workspace members when a
/// new RiskFlag is created. Fire-and-forget: failures are logged but not
/// propagated so the flag insert isn't rolled back.
pub async fn dispatch_new_flag(state: &AppState, flag: &RiskFlag, deep_link: &str) {
    let members = match fetch_member_emails(&state.pool, flag.workspace_id).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("notify: failed to fetch members for {}: {e}", flag.workspace_id);
            return;
        }
    };

    for (user_id, email) in &members {
        // In-app notification row.
        if let Err(e) = insert_notification(
            &state.pool,
            flag.workspace_id,
            *user_id,
            &flag.title,
            &format!("A new {:?} risk flag (severity: {:?}) requires triage.", flag.kind, flag.severity),
            deep_link,
        )
        .await
        {
            tracing::warn!("notify: failed to insert notification for {email}: {e}");
        }

        // Email.
        let mailer = state.mailer.clone();
        let base_url = state.base_url.clone();
        let email = email.clone();
        let title = flag.title.clone();
        let link = format!("{base_url}{deep_link}");
        tokio::spawn(async move {
            if let Err(e) = send_flag_email(&mailer, &email, &title, &link).await {
                tracing::warn!("notify: email to {email} failed: {e}");
            }
        });
    }
}

// ─── Internal helpers ────────────────────────────────────────────────────────

async fn fetch_member_emails(
    pool: &sqlx::PgPool,
    workspace_id: uuid::Uuid,
) -> Result<Vec<(uuid::Uuid, String)>, AppError> {
    let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        r#"
        SELECT u.id, u.email
        FROM workspace_members wm
        JOIN users u ON u.id = wm.user_id
        WHERE wm.workspace_id = $1
        "#,
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn insert_notification(
    pool: &sqlx::PgPool,
    workspace_id: uuid::Uuid,
    user_id: uuid::Uuid,
    title: &str,
    body: &str,
    deep_link: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO notifications (workspace_id, user_id, kind, title, body, deep_link)
        VALUES ($1, $2, 'new_risk_flag', $3, $4, $5)
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(title)
    .bind(body)
    .bind(deep_link)
    .execute(pool)
    .await?;
    Ok(())
}

async fn send_flag_email(
    mailer: &lettre::AsyncSmtpTransport<lettre::Tokio1Executor>,
    to_email: &str,
    flag_title: &str,
    deep_link_url: &str,
) -> anyhow::Result<()> {
    let body = format!(
        "A new risk flag requires your attention.\n\n\"{flag_title}\"\n\nView and triage it here:\n{deep_link_url}"
    );

    let email = EmailMessage::builder()
        .from("CardGuard <noreply@cardguard.app>".parse()?)
        .to(to_email.parse()?)
        .subject(format!("CardGuard: new risk flag — {flag_title}"))
        .header(ContentType::TEXT_PLAIN)
        .body(body)?;

    mailer.send(email).await?;
    Ok(())
}
