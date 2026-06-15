//! Stolen-card reporting screens.
//!
//! Routes:
//!   /stolen/report         — Submit a new report
//!   /stolen/my-reports     — My submitted reports
//!   /stolen/queue          — Platform-moderator queue
//!   /stolen/dispute/:id    — Dispute a report

pub mod dispute;
pub mod my_reports;
pub mod queue;
pub mod report;

pub use dispute::DisputePage;
pub use my_reports::MyReports;
pub use queue::ModeratorQueue;
pub use report::ReportForm;
