pub mod types;

use slog::Logger;
use futures::prelude::*;
use futures::future;
use tokio_core::reactor::Handle;
use reqwest::unstable::async::Client;
use reqwest::{self, Url};
use reqwest::unstable::async;
use failure::Error;
use regex::Regex;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct ReportConfig {
    pub job_name: String,
    pub path: String,
    pub format: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct RepoConfig {
    pub disabled: Option<bool>,
    pub merge_request_title_pattern: Option<String>,
    pub merge_request_title_error: Option<String>,

    #[serde(default)]
    pub reports: Vec<ReportConfig>,
}

impl RepoConfig {
    pub fn is_disabled(&self) -> bool {
        self.disabled.unwrap_or(false)
    }

    pub fn merge_request_title_regex(&self) -> Option<Regex> {
        if let Some(pattern) = self.merge_request_title_pattern.as_ref() {
            if let Ok(re) = Regex::new(pattern) {
                return Some(re);
            }
        }
        None
    }
}

/// Gitlab client struct.
#[derive(Clone)]
pub struct Gitlab {
    endpoint: Url,
    token: String,
    log: Logger,
    client: Client,
}

impl Gitlab {
    pub fn new<U>(url: U, token: String, log: Logger, handle: &Handle) -> Result<Self, Error>
    where
        U: reqwest::IntoUrl,
    {
        Ok(Gitlab {
            endpoint: url.into_url()?,
            log,
            client: Client::new(handle),
            token,
        })
    }

    fn auth_headers(&self) -> reqwest::header::Headers {
        let mut h = reqwest::header::Headers::new();
        h.set_raw("Private-Token".to_string(), self.token.clone());
        h
    }

    fn build_url<S: AsRef<str>>(&self, path: S) -> Url {
        let path = path.as_ref();
        if path.chars().next() == Some('/') {
            panic!("Invalid path: must not start with /");
        }
        let path = format!("/api/v4/{}", path);
        let url = self.endpoint.join(&path).unwrap();
        url
    }

    #[async]
    fn send(self, mut req: async::RequestBuilder) -> Result<async::Response, reqwest::Error> {
        req.headers(self.auth_headers());
        let req = req.build()?;
        trace!(self.log, "http_request";
            "method" => req.method().to_string(),
            "url" => req.url().to_string(),
        );
        let res = await!(
            self.client
                .execute(req)
                .and_then(|res| res.error_for_status())
        )?;
        Ok(res)
    }

    fn get_url(&self, url: Url) -> async::RequestBuilder {
        self.client.get(url)
    }

    fn get(&self, path: String) -> async::RequestBuilder {
        let url = self.build_url(path);
        self.get_url(url)
    }

    fn post<S: AsRef<str>>(&self, path: S) -> async::RequestBuilder {
        self.client.post(self.build_url(path))
    }

    fn put<S: AsRef<str>>(&self, path: S) -> async::RequestBuilder {
        let mut b = self.client.put(self.build_url(path));
        b.headers(self.auth_headers());
        b
    }

    #[async]
    fn delete(self, path: String) -> Result<(), reqwest::Error> {
        let mut req = self.client.delete(self.build_url(path));
        await!(self.send(req))?;
        Ok(())
    }

    #[async]
    fn get_json<S>(self, path: String) -> Result<S, reqwest::Error>
    where
        S: ::serde::de::DeserializeOwned + 'static,
    {
        let req = self.get(path);
        let data = await!(self.send(req).and_then(|mut res| res.json()))?;
        Ok(data)
    }

    /// Load a single page of a paginated list.
    #[async]
    fn load_page<S>(self, url: Url, page: u64) -> Result<Vec<S>, reqwest::Error>
    where
        S: ::serde::de::DeserializeOwned + 'static,
    {
        let mut url = url.clone();
        url.query_pairs_mut().append_pair("page", &page.to_string());

        let req = self.client.get(url);
        let mut res = await!(self.send(req))?;
        let items = await!(res.json())?;
        Ok(items)
    }

    /// Load all pages of a paginated list.
    #[async]
    fn load_paginated<S>(self, path: String, max_pages: Option<u64>) -> Result<Vec<S>, Error>
    where
        S: ::serde::de::DeserializeOwned + 'static,
    {
        let url = self.build_url(path);

        let req = self.client.get(url.clone());
        let mut res = await!(self.clone().send(req))?;

        let total_pages = res.headers()
            .get_raw("x-total-pages")
            .and_then(|x| x.one())
            .ok_or(format_err!("Missing x-total-pages header"))
            .and_then(|raw| -> Result<u64, Error> {
                let x: u64 = ::std::str::from_utf8(raw)?.parse()?;
                Ok(x)
            })?;

        let mut items = await!(res.json::<Vec<S>>())?;

        if total_pages < 2 || max_pages.map(|x| x > 2).unwrap_or(false) {
            // Only one page, no extra work needed.
            return Ok(items);
        }

        let last_page = if let Some(x) = max_pages {
            x
        } else {
            total_pages
        };

        for page in 2..last_page + 1 {
            let page_items = await!(self.clone().load_page(url.clone(), page))?;
            items.extend(page_items.into_iter());
        }

        Ok(items)
    }

    /// Load information for the current user.
    #[async]
    pub fn user(self) -> Result<types::User, reqwest::Error> {
        let u = await!(self.get_json("user".to_string()))?;
        Ok(u)
    }

    /// Load project.
    #[async]
    pub fn project(self, id: u64) -> Result<types::Project, reqwest::Error> {
        let u = await!(self.get_json(format!("projects/{}", id)))?;
        Ok(u)
    }

    /// Get branch info for a branch.
    #[async]
    pub fn branch(self, pid: u64, branch: String) -> Result<types::Branch, reqwest::Error> {
        let path = format!("projects/{}/repository/branches/{}", pid, branch);
        let data = await!(self.get_json(path))?;
        Ok(data)
    }

    #[async]
    pub fn commits(self, pid: u64, branch: String, max: u64) -> Result<Vec<types::Commit>, Error> {
        let max_pages = (max as f64 / 100.0).ceil() as u64;

        let path = format!("projects/{}/repository/commits?ref={}", pid, branch);
        let data = await!(self.load_paginated(path, Some(max_pages)))?;
        Ok(data)
    }

    /// Load a file from a repository.
    #[async]
    pub fn repo_file(
        self,
        pid: u64,
        path: String,
        branch: String,
    ) -> Result<Vec<u8>, reqwest::Error> {
        let path = format!(
            "projects/{}/repository/files/{}/raw?ref={}",
            pid, path, branch
        );
        let req = self.get(path);
        let res = await!(self.send(req))?;
        let body = await!(res.into_body().fold(Vec::<u8>::new(), |mut a, b| {
            a.extend_from_slice(&b[..]);
            future::ok::<_, reqwest::Error>(a)
        }))?;
        Ok(body)
    }

    /// Get a list of merge requests.
    #[async]
    pub fn merge_requests(self) -> Result<Vec<types::MergeRequest>, Error> {
        let path = "merge_requests?scope=all&state=opened".to_string();
        let items = await!(self.load_paginated(path, None))?;
        Ok(items)
    }

    /// Get all jobs of a pipeline.
    #[async]
    pub fn pipeline_jobs(
        self,
        pid: u64,
        pipeline_id: u64,
    ) -> Result<Vec<types::Job>, reqwest::Error> {
        let path = format!("projects/{}/pipelines/{}/jobs", pid, pipeline_id);
        let jobs = await!(self.get_json(path))?;
        Ok(jobs)
    }

    /// Get a single file from an artifact.
    #[async]
    pub fn job_artifact_file(
        self,
        pid: u64,
        job_id: u64,
        path: String,
    ) -> Result<Vec<u8>, reqwest::Error> {
        let path = format!("projects/{}/jobs/{}/artifacts/{}", pid, job_id, path);
        let req = self.get(path);
        let data = await!(self.send(req).and_then(|res| res.into_body().fold(
            Vec::<u8>::new(),
            |mut a, b| {
                a.extend_from_slice(&b[..]);
                future::ok::<_, reqwest::Error>(a)
            }
        )))?;
        Ok(data)
    }

    /// Get the trace log for a CI job.
    #[async]
    pub fn job_trace(self, pid: u64, job_id: u64) -> Result<String, Error> {
        let path = format!("projects/{}/jobs/{}/trace", pid, job_id);
        let req = self.get(path);
        let log = await!(
            self.send(req)
                .and_then(|res| res.into_body().fold(Vec::<u8>::new(), |mut a, b| {
                    a.extend_from_slice(&b[..]);
                    future::ok::<_, reqwest::Error>(a)
                }))
                .map_err(Error::from)
                .and_then(|data| ::std::str::from_utf8(&data)
                    .map_err(Error::from)
                    .map(|x| x.to_string()))
        )?;
        Ok(log)
    }

    /// Get the comments of a merge request.
    #[async]
    pub fn merge_request_commits(self, pid: u64, mrid: u64) -> Result<Vec<types::Commit>, Error> {
        let path = format!("projects/{}/merge_requests/{}/commits", pid, mrid);
        let comments = await!(self.load_paginated(path, None))?;
        Ok(comments)
    }

    /// Get the comments of a merge request.
    #[async]
    pub fn merge_request_comments(self, pid: u64, mrid: u64) -> Result<Vec<types::Note>, Error> {
        let path = format!("projects/{}/merge_requests/{}/notes", pid, mrid);
        let comments = await!(self.load_paginated(path, None))?;
        Ok(comments)
    }

    /// Get list of pipelines for a merge request.
    #[async]
    pub fn merge_request_pipelines(
        self,
        pid: u64,
        mrid: u64,
    ) -> Result<Vec<types::Pipeline>, reqwest::Error> {
        let path = format!("projects/{}/merge_requests/{}/pipelines", pid, mrid);
        let pipelines = await!(self.get_json(path))?;
        Ok(pipelines)
    }

    /// Create a new merge request comment.
    #[async]
    pub fn merge_request_comment_create(
        self,
        pid: u64,
        mrid: u64,
        body: String,
    ) -> Result<(), reqwest::Error> {
        let path = format!("projects/{}/merge_requests/{}/notes", pid, mrid);
        let mut b = self.post(path);
        b.json(&json!({
            "body": body,
        }));

        await!(self.send(b))?;
        Ok(())
    }

    #[async]
    pub fn merge_request_comment_update(
        self,
        pid: u64,
        mrid: u64,
        note_id: u64,
        body: String,
    ) -> Result<(), reqwest::Error> {
        let path = format!("projects/{}/merge_requests/{}/notes/{}", pid, mrid, note_id);

        let mut b = self.put(path);
        b.json(&json!({
            "body": body,
        }));
        await!(self.clone().send(b))?;
        Ok(())
    }

    #[async]
    pub fn merge_request_comment_delete(
        self,
        pid: u64,
        mrid: u64,
        note_id: u64,
    ) -> Result<(), reqwest::Error> {
        let path = format!("projects/{}/merge_requests/{}/notes/{}", pid, mrid, note_id);

        await!(self.delete(path))?;

        Ok(())
    }

    #[async]
    pub fn full_merge_request(
        self,
        mr: types::MergeRequest,
        bot_id: u64,
    ) -> Result<types::FullMergeRequest, Error> {
        // Load repo config.
        let repo_config_res = await!(self.clone().repo_file(
            mr.project_id,
            ".gitlab-bot.toml".to_string(),
            "master".to_string()
        )).map_err(Error::from)
            .and_then(|data| ::toml::from_slice::<RepoConfig>(&data).map_err(Error::from));
        let repo_config = match repo_config_res {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Could not load repo config: {}", e);
                RepoConfig::default()
            }
        };

        let project = await!(self.clone().project(mr.project_id))?;

        // Load source branch.
        let source_branch = await!(self.clone().branch(mr.project_id, mr.source_branch.clone()))?;

        let source_branch_commits = await!(self.clone().commits(
            mr.project_id,
            mr.source_branch.clone(),
            100
        ))?;

        let target_branch_commits = await!(self.clone().commits(
            mr.project_id,
            mr.target_branch.clone(),
            100
        ))?;

        // Load comments.
        let comments = await!(self.clone().merge_request_comments(mr.project_id, mr.iid))?;
        let bot_comments = comments
            .iter()
            .filter(|c| c.author.as_ref().map(|a| a.id == bot_id).unwrap_or(false))
            .map(|x| x.clone())
            .collect::<Vec<_>>();

        let pipelines = await!(self.clone().merge_request_pipelines(mr.project_id, mr.iid))?;

        Ok(types::FullMergeRequest {
            project,
            request: mr,
            source_branch,
            source_branch_commits,
            target_branch_commits: target_branch_commits,
            comments,
            bot_comments,
            pipelines,
            repo_config,
        })
    }
}
