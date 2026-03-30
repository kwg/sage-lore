// SPDX-License-Identifier: MIT
//! Forgejo API response types for deserialization.

use super::types::*;
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Forgejo issue representation (API response).
#[derive(Debug, Deserialize)]
pub struct ForgejoIssue {
    pub number: i64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub labels: Option<Vec<ForgejoLabel>>,
    pub milestone: Option<ForgejoMilestone>,
    pub assignees: Option<Vec<ForgejoUser>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub html_url: String,
}

/// Forgejo label representation.
#[derive(Debug, Deserialize)]
pub struct ForgejoLabel {
    pub name: String,
}

/// Forgejo user representation.
#[derive(Debug, Deserialize)]
pub struct ForgejoUser {
    pub login: String,
}

/// Forgejo milestone representation (API response).
#[derive(Debug, Deserialize)]
pub struct ForgejoMilestone {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub due_on: Option<DateTime<Utc>>,
    pub open_issues: i64,
    pub closed_issues: i64,
}

/// Forgejo comment representation (API response).
#[derive(Debug, Deserialize)]
pub struct ForgejoComment {
    pub id: i64,
    pub body: String,
    pub user: ForgejoUser,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Forgejo pull request representation (API response).
#[derive(Debug, Deserialize)]
pub struct ForgejoPullRequest {
    pub number: i64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub head: ForgejoPrRef,
    pub base: ForgejoPrRef,
    pub mergeable: Option<bool>,
    pub merged: bool,
    pub merged_at: Option<DateTime<Utc>>,
    pub html_url: String,
    pub diff_url: String,
}

/// Forgejo PR branch reference.
#[derive(Debug, Deserialize)]
pub struct ForgejoPrRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
}

impl From<ForgejoIssue> for Issue {
    fn from(fi: ForgejoIssue) -> Self {
        Issue {
            number: fi.number,
            title: fi.title,
            body: fi.body.unwrap_or_default(),
            state: fi.state,
            labels: fi
                .labels
                .map(|l| l.into_iter().map(|label| label.name).collect())
                .unwrap_or_default(),
            milestone_id: fi.milestone.as_ref().map(|m| m.id),
            assignees: fi
                .assignees
                .map(|a| a.into_iter().map(|u| u.login).collect())
                .unwrap_or_default(),
            created_at: fi.created_at,
            updated_at: fi.updated_at,
            closed_at: fi.closed_at,
            html_url: fi.html_url,
        }
    }
}

impl From<ForgejoMilestone> for Milestone {
    fn from(fm: ForgejoMilestone) -> Self {
        Milestone {
            id: fm.id,
            title: fm.title,
            description: fm.description.unwrap_or_default(),
            state: fm.state,
            due_on: fm.due_on,
            open_issues: fm.open_issues,
            closed_issues: fm.closed_issues,
        }
    }
}

impl From<ForgejoComment> for Comment {
    fn from(fc: ForgejoComment) -> Self {
        Comment {
            id: fc.id,
            body: fc.body,
            user: fc.user.login,
            created_at: fc.created_at,
            updated_at: fc.updated_at,
        }
    }
}

impl From<ForgejoPullRequest> for PullRequest {
    fn from(fp: ForgejoPullRequest) -> Self {
        PullRequest {
            number: fp.number,
            title: fp.title,
            body: fp.body.unwrap_or_default(),
            state: if fp.merged {
                "merged".to_string()
            } else {
                fp.state
            },
            head: PrBranch {
                ref_name: fp.head.ref_name,
                sha: fp.head.sha,
            },
            base: PrBranch {
                ref_name: fp.base.ref_name,
                sha: fp.base.sha,
            },
            mergeable: fp.mergeable,
            merged: fp.merged,
            merged_at: fp.merged_at,
            html_url: fp.html_url,
            diff_url: fp.diff_url,
        }
    }
}
