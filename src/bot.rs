use failure::Error;
use tokio_core::reactor::Handle;
use slog::Logger;
use futures::prelude::*;
use futures::future;
use futures::stream;
use chrono::{Duration, Utc};

use client;
use client::types;

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
            interval: 10,
        })
    }
}

#[derive(Clone)]
pub struct Bot {
    config: Config,
    handle: Handle,
    log: Logger,
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
    fn process_merge_request_reminder(self, mr: types::FullMergeRequest) -> Result<(), Error> {
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
        let project_id = mr.project_id;
        let mr = await!(self.client.clone().full_merge_request(mr, bot.id))?;

        if mr.repo_config.is_disabled() {
            return Ok(());
        }

        // If the MR has not been updated for X days, post a reminder comment.
        await!(self.clone().process_merge_request_reminder(mr.clone()))?;

        let mut msg = String::new();

        let mut validation_msg = String::new();

        // If configured, validate the merge request title.
        if let Some(re) = mr.repo_config.merge_request_title_regex() {
            let is_valid = re.is_match(&mr.request.title);
            if !is_valid {
                // Check if warning is needed.
                let has_warning = mr.has_bot_comment("[title_warning]", None);
                if !has_warning {
                    // No warning present, so post a comment.
                    let err = mr.repo_config
                        .merge_request_title_error
                        .clone()
                        .unwrap_or(format!(
                            "Merge request title must match the pattern: `{}`",
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
        let user = await!(self.client.clone().user())?;

        // Load merge requests.
        let mrs = await!(self.client.clone().merge_requests())?;

        let bot = self.clone();
        let f = stream::iter_ok(mrs)
            .map(move |mr| {
                trace!(bot.log.clone(), "merge_request_check";
                    "mr_name" => mr.title.clone(),
                    "mr_id" => mr.iid,
                );
                let log = bot.log.clone();
                let res = bot.clone().process_merge_request(user.clone(), mr.clone());
                res.then(move |res| {
                    match res {
                        Ok(_) => {
                            debug!(log, "merge_request_complete";
                                "mr_name" => mr.title.clone(),
                                "mr_id" => mr.iid,
                            );
                        }
                        Err(e) => {
                            error!(log, "merge_request_failed";
                                "mr_name" => mr.title.clone(),
                                "mr_id" => mr.iid,
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
            await!(self.clone().process()).ok();
            await!(::tokio_core::reactor::Timeout::new(interval, &self.handle)?).ok();
        }
    }

    pub fn run(&self) -> Box<Future<Item = (), Error = Error>> {
        Box::new(self.clone().do_loop())
    }
}
