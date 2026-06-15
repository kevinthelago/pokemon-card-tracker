//! Stolen-card reporting screens.
//!
//! Routes (all nested under the main `<Routes>`):
//!   `/stolen/report`           — Submit a new stolen-card report
//!   `/stolen/my-reports`       — Current user's submitted reports
//!   `/stolen/queue`            — Platform-moderator moderation queue
//!   `/stolen/dispute/:id`      — Dispute a report (for the accused owner)

pub mod dispute;
pub mod my_reports;
pub mod queue;
pub mod report;

pub use dispute::DisputePage;
pub use my_reports::MyReports;
pub use queue::ModeratorQueue;
pub use report::ReportForm;
