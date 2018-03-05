use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
    pub id: u64,
    pub username: String,
    pub email: String,
    pub name: Option<String>,
    pub state: String,
    pub avatar_url: Option<String>,
    pub web_url: Option<String>,
    pub created_at: String,
    pub bio: Option<Value>,
    pub location: Option<Value>,
    pub skype: Option<String>,
    pub linkedin: Option<String>,
    pub twitter: Option<String>,
    pub website_url: Option<String>,
    pub organization: Option<String>,
    pub last_sign_in_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub theme_id: u64,
    pub last_activity_on: Option<DateTime<Utc>>,
    pub color_scheme_id: u64,
    pub projects_limit: u64,
    pub current_sign_in_at: Option<DateTime<Utc>>,
    /*
    identities: [
    {pub provider: String,},
    {pub provider: String,},
    {pub provider: String,}
    ],
    */
    pub can_create_group: bool,
    pub can_create_project: bool,
    pub two_factor_enabled: bool,
    pub external: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Author {
    pub id: u64,
    pub username: String,
    pub email: Option<String>,
    pub name: String,
    pub state: String,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Assignee {
    pub id: u64,
    pub username: String,
    pub email: Option<String>,
    pub name: String,
    pub state: String,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub id: u64,
    pub description: Option<String>,
    pub default_branch: String,
    pub visibility: String,
    pub ssh_url_to_repo: String,
    pub http_url_to_repo: String,
    pub web_url: String,
    #[serde(default)]
    pub tag_list: Vec<String>,
    /*
  owner: {
    id: i64,
    name: String,
    created_at: "2013-09-30T13:46:02Z"
  },
  */
    pub name: String,
    pub name_with_namespace: String,
    pub path: String,
    pub path_with_namespace: String,
    pub issues_enabled: bool,
    pub open_issues_count: i64,
    pub merge_requests_enabled: bool,
    pub jobs_enabled: bool,
    pub wiki_enabled: bool,
    pub snippets_enabled: bool,
    pub resolve_outdated_diff_discussions: Option<bool>,
    pub container_registry_enabled: bool,
    pub created_at: String,
    pub last_activity_at: String,
    pub creator_id: i64,
    /*
  namespace: {
    id: i64,
    name: String,
    path: String,
    kind: String,
    full_path: "diaspora"
  },
  */
    pub import_status: String,
    pub import_error: Option<Value>,
    /*
  permissions: {
    project_access: {
      access_level: i64,
      notification_level: 3
    },
    group_access: {
      access_level: i64,
      notification_level: 3
    }
  },
  */
    pub archived: bool,
    pub avatar_url: Option<String>,
    pub shared_runners_enabled: bool,
    pub forks_count: i64,
    pub star_count: i64,
    pub runners_token: Option<String>,
    pub public_jobs: bool,
    /*
  shared_with_groups: [
    {
      group_id: i64,
      group_name: String,
      group_access_level: 30
    },

    {
      group_id: i64,
      group_name: String,
      group_access_level: 10
    }
  ],
  */
    pub repository_storage: Option<String>,
    pub only_allow_merge_if_pipeline_succeeds: bool,
    pub only_allow_merge_if_all_discussions_are_resolved: bool,
    pub printing_merge_requests_link_enabled: Option<bool>,
    pub request_access_enabled: bool,
    pub approvals_before_merge: Option<i64>,
    /*
  statistics: {
    commit_count: i64,
    storage_size: i64,
    repository_size: i64,
    lfs_objects_size: i64,
    job_artifacts_size: 0
  },
  _links: {
    self: String,
    issues: String,
    merge_requests: String,
    repo_branches: String,
    labels: String,
    events: String,
    members: "http://example.com/api/v4/projects/1/members"
  }
  */
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Milestone {
    pub id: u64,
    pub iid: u64,
    pub project_id: u64,
    pub title: String,
    pub description: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub due_date: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Commit {
    pub author_email: String,
    pub author_name: String,
    pub authored_date: DateTime<Utc>,
    pub committed_date: DateTime<Utc>,
    pub committer_email: String,
    pub committer_name: String,
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub message: String,
    pub parent_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Branch {
    pub name: String,
    pub merged: bool,
    pub protected: bool,
    pub developers_can_push: bool,
    pub developers_can_merge: bool,
    pub commit: Commit,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MergeRequestTimeStats {
    pub time_estimate: u64,
    pub total_time_spent: u64,
    pub human_time_estimate: Option<String>,
    pub human_total_time_spent: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MergeRequest {
    pub id: u64,
    pub iid: u64,
    pub target_branch: String,
    pub source_branch: String,
    pub project_id: u64,
    pub title: String,
    pub state: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub upvotes: u64,
    pub downvotes: u64,
    pub author: Author,
    pub assignee: Option<Assignee>,
    pub source_project_id: u64,
    pub target_project_id: u64,
    pub labels: Vec<String>,
    pub description: String,
    pub work_in_progress: bool,
    pub milestone: Option<Milestone>,
    pub merge_when_pipeline_succeeds: bool,
    pub merge_status: String,
    pub sha: String,
    pub merge_commit_sha: Option<String>,
    pub user_notes_count: u64,
    pub changes_count: Option<String>,
    pub should_remove_source_branch: Option<bool>,
    pub force_remove_source_branch: bool,
    pub web_url: String,
    pub time_stats: MergeRequestTimeStats,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    pub id: u64,
    pub body: String,
    pub attachment: Option<Value>,
    pub author: Option<Author>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub system: Option<bool>,
    pub noteable_id: u64,
    pub noteable_type: String,
    pub noteable_iid: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pipeline {
    pub id: u64,
    pub sha: String,
    #[serde(rename = "ref")]
    pub branch: String,
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JobArtifact {
    pub filename: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Job {
    pub commit: Commit,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub id: u64,
    pub name: String,
    pub status: String,
    pub tag: bool,
    pub stage: String,
    #[serde(rename = "ref")]
    pub branch: String,
    pub artifacts_file: Option<JobArtifact>,
    /*
    coverage: null,
    pipeline: {
      id: 6,
      ref: "master",
      sha: "0ff3ae198f8601a285adcf5c0fff204ee6fba5fd",
      status: "pending"
    },
    runner: null,
    user: {
      avatar_url: "http://www.gravatar.com/avatar/e64c7d89f26bd1972efa854d13d7dd61?s=80&d=identicon",
      bio: null,
      created_at: "2015-12-21T13:14:24.077Z",
      id: 1,
      linkedin: "",
      name: "Administrator",
      skype: "",
      state: "active",
      twitter: "",
      username: "root",
      web_url: "http://gitlab.dev/root",
      website_url: ""
    }
  },
  */
}
