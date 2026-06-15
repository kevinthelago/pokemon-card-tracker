use lettre::{
    message::header::ContentType, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

use crate::error::AppError;

pub async fn send_verification_email(
    mailer: &AsyncSmtpTransport<Tokio1Executor>,
    from: &str,
    to: &str,
    verify_url: &str,
) -> Result<(), AppError> {
    let body = format!(
        "Welcome to CardGuard!\n\nVerify your email address:\n{verify_url}\n\nThis link expires in 24 hours."
    );
    let msg = Message::builder()
        .from(from.parse().map_err(|e| AppError::Other(anyhow::anyhow!("bad from addr: {e}")))?)
        .to(to.parse().map_err(|e| AppError::Other(anyhow::anyhow!("bad to addr: {e}")))?)
        .subject("Verify your CardGuard email")
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .map_err(|e| AppError::Other(anyhow::anyhow!("build email: {e}")))?;
    mailer
        .send(msg)
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!("send email: {e}")))?;
    Ok(())
}

pub async fn send_password_reset_email(
    mailer: &AsyncSmtpTransport<Tokio1Executor>,
    from: &str,
    to: &str,
    reset_url: &str,
) -> Result<(), AppError> {
    let body = format!(
        "Someone requested a password reset for your CardGuard account.\n\nReset your password:\n{reset_url}\n\nThis link expires in 1 hour. If you didn't request this, you can ignore this email."
    );
    let msg = Message::builder()
        .from(from.parse().map_err(|e| AppError::Other(anyhow::anyhow!("bad from addr: {e}")))?)
        .to(to.parse().map_err(|e| AppError::Other(anyhow::anyhow!("bad to addr: {e}")))?)
        .subject("Reset your CardGuard password")
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .map_err(|e| AppError::Other(anyhow::anyhow!("build email: {e}")))?;
    mailer
        .send(msg)
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!("send email: {e}")))?;
    Ok(())
}
