use std::collections::HashMap;
use std::borrow::Borrow;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use failure::Error;
use tokio_core::reactor::Handle;
use slog::Logger;
use futures::prelude::*;
use futures::future;
use futures::stream;
use chrono::{Duration, Utc};
use regex::Regex;

use client;
use client::types;

#[derive(Clone, Debug)]
pub struct FullMergeRequest {
    pub project: types::Project,
    pub request: types::MergeRequest,
    pub source_branch: types::Branch,
    pub source_branch_commits: Vec<types::Commit>,
    pub target_branch_commits: Vec<types::Commit>,
    pub comments: Vec<types::Note>,
    pub bot_comments: Vec<types::Note>,
    pub pipelines: Vec<types::Pipeline>,

    pub repo_config: RepoConfig,
}

impl FullMergeRequest {
    pub fn has_bot_comment(&self, marker: &str, max_age_days: Option<i64>) -> bool {
        let now = Utc::now();
        self.bot_comments
            .iter()
            .find(|c| {
                // If max age is set, drop all older comments.
                if let Some(days) = max_age_days.clone() {
                    if c.created_at < now - Duration::days(days) {
                        return false;
                    }
                }
                // Only check comments which contain marker.
                if c.body.contains(marker) == false {
                    return false;
                }
                true
            })
            .is_some()
    }

    pub fn job_url(&self, job_id: u64) -> String {
        format!("{}/-/jobs/{}", self.project.web_url, job_id)
    }
}

struct CacheItem<T> {
    item: T,
    valid_until: Option<Instant>,
}

#[derive(Default)]
struct Cacher<K: ::std::hash::Hash + ::std::cmp::Eq, V> {
    items: HashMap<K, CacheItem<V>>,
}

impl<K: ::std::cmp::Eq + ::std::hash::Hash, V> Cacher<K, V> {
    fn get(&self, id: &K) -> Option<&V> {
        self.items
            .get(id)
            .and_then(|i| match i.valid_until.as_ref() {
                Some(x) if x >= &Instant::now() => Some(&i.item),
                _ => None,
            })
    }

    fn add(&mut self, id: K, value: V, valid_until: Option<Instant>) {
        self.items.insert(
            id,
            CacheItem {
                item: value,
                valid_until,
            },
        );
    }
}

#[derive(Default)]
struct CacheInner {
    merge_requests: HashMap<u64, FullMergeRequest>,
    project_configs: Cacher<u64, RepoConfig>,
}

#[derive(Clone)]
struct Cache(Arc<Mutex<CacheInner>>);

impl Cache {
    fn new() -> Self {
        Cache(Arc::new(Mutex::new(CacheInner::default())))
    }

    fn merge_request_changed(&self, mr: &types::MergeRequest) -> bool {
        let b = self.0.lock().unwrap();
        match b.merge_requests.get(&mr.id) {
            Some(ref x) if x.request.updated_at == mr.updated_at => false,
            _ => true,
        }
    }

    fn get_merge_request(&self, mr: types::MergeRequest) -> Option<FullMergeRequest> {
        let b = self.0.lock().unwrap();
        match b.merge_requests.get(&mr.id) {
            Some(ref x) if x.request.updated_at == mr.updated_at => Some((*x).clone()),
            _ => None,
        }
    }

    fn set_merge_request(&self, mr: FullMergeRequest) {
        let mut b = self.0.lock().unwrap();
        b.merge_requests.insert(mr.request.id, mr);
    }

    fn get_project_config(&self, project_id: u64) -> Option<RepoConfig> {
        let b = self.0.lock().unwrap();
        b.project_configs.get(&project_id).map(|x| x.clone())
    }

    fn set_project_config(&self, project_id: u64, conf: RepoConfig) {
        let mut b = self.0.lock().unwrap();
        b.project_configs.add(
            project_id,
            conf,
            Some(Instant::now() + ::std::time::Duration::from_secs(60 * 30)),
        );
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct ReportConfig {
    pub job_name: String,
    pub path: String,
    pub format: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct RepoMergeRequestConfig {
    pub title_pattern: Option<String>,
    pub title_error: Option<String>,
    pub branch_name_pattern: Option<String>,
    pub branch_name_error: Option<String>,
}

impl RepoMergeRequestConfig {
    pub fn title_regex(&self) -> Option<Regex> {
        self.title_pattern.as_ref()
            .and_then(|p| Regex::new(p).ok())
    }

    pub fn branch_regex(&self) -> Option<Regex> {
        self.branch_name_pattern.as_ref()
            .and_then(|p| Regex::new(p).ok())
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct RepoConfig {
    pub disabled: Option<bool>,
    pub merge_requests: Option<RepoMergeRequestConfig>,
    #[serde(default)]
    pub reports: Vec<ReportConfig>,
}

impl RepoConfig {
    pub fn is_disabled(&self) -> bool {
        self.disabled.unwrap_or(false)
    }

}

#[derive(Clone)]
pub struct Config {
    endpoint: String,
    token: String,
    interval: u64,
}

impl Config {
    fn from_env() -> Result<Self, Error> {
        use std::env::var;

        let url =
            var("GITLAB_BOT_URL").map_err(|_| format_err!("Missing env var: GITLAB_BOT_URL"))?;

        let token =
            var("GITLAB_BOT_TOKEN").map_err(|_| format_err!("Missing env var: GITLAB_BOT_TOKEN"))?;

        Ok(Config {
            endpoint: url,
            token,
            interval: 60 * 5, // 5 minutes.
        })
    }
}

#[derive(Clone)]
pub struct Bot {
    config: Config,
    handle: Handle,
    log: Logger,
    cache: Cache,
    client: client::Gitlab,
}

impl Bot {
    pub fn new(config: Config, handle: Handle) -> Result<Self, Error> {
        let log = Self::default_logger();
        let c = client::Gitlab::new(&config.endpoint, config.token.clone(), log.clone(), &handle)?;
        Ok(Bot {
            client: c,
            handle,
            log,
            cache: Cache::new(),
            config,
        })
    }

    pub fn from_env(handle: Handle) -> Result<Self, Error> {
        Self::new(Config::from_env()?, handle)
    }

    fn default_logger() -> Logger {
        use sloggers::Build;
        use sloggers::terminal::{Destination, TerminalLoggerBuilder};
        use sloggers::types::Severity;

        let mut builder = TerminalLoggerBuilder::new();
        builder.level(Severity::Trace);
        builder.destination(Destination::Stderr);

        let logger = builder.build().unwrap();

        logger
    }

    #[async]
    fn cached_repo_config(self, project: types::Project) -> Result<RepoConfig, Error> {
        if let Some(conf) = self.cache.get_project_config(project.id) {
            // Found cached version.
            Ok(conf)
        } else {
            // No cached version, actually load it.
            // Load repo config.
            let repo_config_res = await!(self.client.clone().repo_file(
                project.id,
                ".gitlab-bot.toml".to_string(),
                "master".to_string()
            )).map_err(Error::from)
                .and_then(|data| ::toml::from_slice::<RepoConfig>(&data).map_err(Error::from));

            let config = match repo_config_res {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Could not load repo config: {}", e);
                    RepoConfig::default()
                }
            };

            self.cache.set_project_config(project.id, config.clone());

            Ok(config)
        }
    }

    #[async]
    pub fn full_merge_request(
        self,
        mr: types::MergeRequest,
        bot_id: u64,
    ) -> Result<FullMergeRequest, Error> {
        // Check cache first.
        // Cache will match if updated_at did not change.
        if let Some(cached_mr) = self.cache.get_merge_request(mr.clone()) {
            return Ok(cached_mr);
        }

        let project = await!(self.client.clone().project(mr.project_id))?;
        let repo_config = await!(self.clone().cached_repo_config(project.clone()))?;

        // Load source branch.
        let source_branch = await!(
            self.client
                .clone()
                .branch(mr.project_id, mr.source_branch.clone())
        )?;

        let source_branch_commits = await!(self.client.clone().commits(
            mr.project_id,
            mr.source_branch.clone(),
            100
        ))?;

        let target_branch_commits = await!(self.client.clone().commits(
            mr.project_id,
            mr.target_branch.clone(),
            100
        ))?;

        // Load comments.
        let comments = await!(
            self.client
                .clone()
                .merge_request_comments(mr.project_id, mr.iid)
        )?;
        let bot_comments = comments
            .iter()
            .filter(|c| c.author.as_ref().map(|a| a.id == bot_id).unwrap_or(false))
            .map(|x| x.clone())
            .collect::<Vec<_>>();

        let pipelines = await!(
            self.client
                .clone()
                .merge_request_pipelines(mr.project_id, mr.iid)
        )?;

        let full = FullMergeRequest {
            project,
            request: mr,
            source_branch,
            source_branch_commits,
            target_branch_commits: target_branch_commits,
            comments,
            bot_comments,
            pipelines,
            repo_config,
        };

        self.cache.set_merge_request(full.clone());
        Ok(full)
    }

    #[async]
    fn process_merge_request_reminder(self, mr: FullMergeRequest) -> Result<(), Error> {
        // Check if time reminder is needed.
        let now = Utc::now();
        let reminder_days = 5;
        if mr.source_branch.commit.committed_date < now - Duration::days(reminder_days) {
            let has_reminder = mr.has_bot_comment("[reminder]", Some(reminder_days));

            if has_reminder == false {
                // Create a new reminder comment.

                let body = format!(
                    "@{} friendly reminder: this merge request has not been updated for {} days!\nLet's get going! ;)\n\n[reminder]",
                    mr.request.author.username, reminder_days);

                await!(self.client.clone().merge_request_comment_create(
                    mr.request.project_id,
                    mr.request.iid,
                    body
                ))?;
            }
        }

        Ok(())
    }

    #[async]
    fn process_merge_request(self, bot: types::User, mr: types::MergeRequest) -> Result<(), Error> {
        trace!(self.log, "process_merge_request_start";
            "project_id" => mr.project_id,
            "merge_request_id" => mr.id,
        );
        let project_id = mr.project_id;
        let updated_at = mr.updated_at.clone();

        let mr = await!(self.clone().full_merge_request(mr, bot.id))?;

        if mr.repo_config.is_disabled() {
            return Ok(());
        }

        // If the MR has not been updated for X days, post a reminder comment.
        await!(self.clone().process_merge_request_reminder(mr.clone()))?;

        let mut msg = String::new();

        let mut validation_msg = String::new();

        if let Some(mr_config) = mr.repo_config.merge_requests.clone() {
            // If configured, validate the merge request title.
            if let Some(re) = mr_config.title_regex() {
                let is_valid = re.is_match(&mr.request.title);
                if !is_valid {
                    // Check if warning is needed.
                    let has_warning = mr.has_bot_comment("[title_warning]", None);
                    if !has_warning {
                        // No warning present, so post a comment.
                        let err = mr_config
                            .title_error
                            .clone()
                            .unwrap_or(format!(
                                "Merge request title should match the pattern: `{}`",
                                re.to_string()
                            ));

                        let comment_body = format!(
                            "@{}\n\nThe merge request title is invalid. \n{}\n\n[title_warning]",
                            mr.request.author.username, err
                        );

                        await!(self.client.clone().merge_request_comment_create(
                            project_id,
                            mr.request.iid,
                            comment_body
                        ))?;
                    }
                }
                validation_msg.push_str(&format!(
                    "- [{}] Valid Merge Request Title{} \n",
                    if is_valid { "x" } else { " " },
                    if is_valid { "" } else { " :warning:" },
                ));
            }


            // If configured, validate the branch name.
            if let Some(re) = mr_config.branch_regex() {
                let is_valid = re.is_match(&mr.source_branch.name);
                if !is_valid {
                    // Check if warning is needed.
                    let has_warning = mr.has_bot_comment("[branch_name_warning]", None);
                    if !has_warning {
                        // No warning present, so post a comment.
                        let err = mr_config
                            .branch_name_error
                            .clone()
                            .unwrap_or(format!(
                                "Branch name should match the pattern: `{}`",
                                re.to_string()
                            ));

                        let comment_body = format!(
                            "@{}\n\nThe branch name is invalid. \n{}\n\n[branch_name_warning]",
                            mr.request.author.username, err
                        );

                        debug!(self.log, "posting_branch_name_warning";
                            "branch_name" => &mr.source_branch.name,
                            "err" => &err,
                        );

                        await!(self.client.clone().merge_request_comment_create(
                            project_id,
                            mr.request.iid,
                            comment_body
                        ))?;
                    }
                }
                validation_msg.push_str(&format!(
                    "- [{}] Valid Branch Name{} \n",
                    if is_valid { "x" } else { " " },
                    if is_valid { "" } else { " :warning:" },
                ));
            }
        }

        if mr.request.assignee.is_none() {
            validation_msg.push_str("- [ ] Reviewer selected :warning: \n");
        } else {
            validation_msg.push_str("- [x] Reviewer selected \n");
        }

        // Check if failed.
        if mr.pipelines.len() > 0 {
            let pipeline = mr.pipelines[0].clone();

            // Found a pipeline.

            // Check jobs.
            let jobs = await!(self.client.clone().pipeline_jobs(project_id, pipeline.id))?;
            let failed_jobs = jobs.iter()
                .filter(|j| j.status == "failed")
                .map(|x| x.clone())
                .collect::<Vec<_>>();
            let success_jobs = jobs.iter()
                .filter(|j| j.status == "success")
                .map(|x| x.clone())
                .collect::<Vec<_>>();

            msg.push_str("## Build Status\n\n");

            /*
            let mut artifacts_info = "";
            if pipelie.status != "pending" {
                for job in &jobs {
                    for report in &mr.config.reports {
                        if report.job_name == job.name {
                            let file_res = await!(self.client.clone()
                                .job_artifact_file(project_id, job.id, report.path.clone())
                            );
                        }
                    }
                }
            }
            */

            if pipeline.status == "failed" {
                // Found a failed pipeline!

                msg.push_str(&format!("Pipeline failed! :warning:\n\n",));

                for job in failed_jobs {
                    // Retrieve log.
                    let trace = await!(self.client.clone().job_trace(project_id, job.id))?;

                    let job_url = mr.job_url(job.id);
                    msg.push_str(&format!(
                        "#### Job: [{}]({})\n\n\
                         <details>\
                         <summary>Show Logs</summary>\
                         <pre><code>{}</code></pre>\
                         </details><br>\n\
                         ",
                        job.name, job_url, trace,
                    ));
                }
            } else if pipeline.status == "success" {
                msg.push_str(&format!(
                    "Pipeline passed! :rocket:\n\n\
                     Successful jobs: {}\n",
                    success_jobs
                        .into_iter()
                        .map(|job| format!("[{}]({})", job.name, mr.job_url(job.id)))
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            } else if pipeline.status == "pending" {
                msg.push_str("Pipeline is running... ")
            }
        }

        if validation_msg != "" {
            msg.push_str(&format!("## Validation\n\n{}\n\n", validation_msg));
        }

        if msg != "" {
            // Msg is non-empty.

            // Append identifier tag.
            msg.push_str(&format!("\n\n[report]\n"));

            let has_changes = mr.bot_comments
                .iter()
                .find(|c| c.body.contains("[report]"))
                .map(|c| c.body != msg)
                .unwrap_or(true);

            if !has_changes {
                // Report did not change, so no need to update.
                return Ok(());
            }

            // Check if the very last message was a build report.
            // If so, we can just update it.
            // NOTE: comments are sorted by date ascendingly, so the newest
            // comment is the first entry.
            let update_id = mr.comments.get(0).and_then(|c| {
                if c.author.as_ref().map(|a| a.id == bot.id).unwrap_or(false)
                    && c.body.contains("[report]")
                {
                    Some(c.id)
                } else {
                    None
                }
            });

            if let Some(id) = update_id {
                await!(self.client.clone().merge_request_comment_update(
                    project_id,
                    mr.request.iid,
                    id,
                    msg
                ))?;
            } else {
                await!(self.client.clone().merge_request_comment_create(
                    project_id,
                    mr.request.iid,
                    msg
                ))?;
            }

            // Delete older build reports.
            for comment in mr.bot_comments.into_iter().skip(1) {
                if comment.body.contains("[report]") {
                    await!(self.client.clone().merge_request_comment_delete(
                        project_id,
                        mr.request.iid,
                        comment.id,
                    ))?;
                }
            }
        }

        Ok(())
    }

    #[async]
    fn process(self) -> Result<(), Error> {
        let log = self.log.clone();
        info!(log, "process_start");

        // Get info about the current user.
        trace!(log, "loading_user");
        let user = await!(self.client.clone().user())?;
        trace!(log, "user_loaded"; "name" => &user.username);

        // Load merge requests.
        let mrs = await!(self.client.clone().merge_requests())?;

        // Filter out unchanged MRs.
        let mrs = mrs.into_iter()
            .filter(|mr| {
                let changed = self.cache.merge_request_changed(mr);
                if !changed {
                    trace!(log, "skipping_unchanged_merge_request";
                        "project_id" => mr.project_id,
                        "merge_request_title" => &mr.title,
                    );
                }
                changed
            })
            .collect::<Vec<_>>();

        let bot = self.clone();
        let f = stream::iter_ok(mrs)
            .map(move |mr| {
                trace!(bot.log.clone(), "merge_request_check";
                    "mr_name" => mr.title.clone(),
                    "mr_id" => mr.id,
                );
                let log = bot.log.clone();
                let res = bot.clone().process_merge_request(user.clone(), mr.clone());
                res.then(move |res| {
                    match res {
                        Ok(_) => {
                            debug!(log, "merge_request_complete";
                                "mr_name" => mr.title.clone(),
                                "mr_id" => mr.id,
                            );
                        }
                        Err(e) => {
                            error!(log, "merge_request_failed";
                                "mr_name" => mr.title.clone(),
                                "mr_id" => mr.id,
                                "error" => e.to_string(),
                            );
                        }
                    }
                    future::ok::<_, Error>(())
                })
            })
            .buffered(5)
            .collect();
        await!(f)?;

        info!(self.log, "process_complete");

        Ok(())
    }

    #[async]
    fn do_loop(self) -> Result<(), Error> {
        let interval = ::std::time::Duration::from_secs(self.config.interval);
        loop {
            trace!(self.log, "running_loop");
            match await!(self.clone().process()) {
                Ok(_) => {}
                Err(e) => {
                    error!(self.log, "processing_failed";
                        "error" => e.to_string(),
                    );
                }
            }
            await!(::tokio_core::reactor::Timeout::new(interval, &self.handle)?).ok();
        }
    }

    pub fn run(&self) -> Box<Future<Item = (), Error = Error>> {
        Box::new(self.clone().do_loop())
    }
}
