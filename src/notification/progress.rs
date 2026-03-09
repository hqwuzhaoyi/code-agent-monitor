use serde::{Deserialize, Serialize};

use crate::session::state::ConversationStateManager;
use crate::team::bridge::{TeamBridge, TeamTaskSummary};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemainingTaskItem {
    pub subject: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressSnapshot {
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub pending_tasks: usize,
    pub in_progress_tasks: usize,
    pub completion_rate: u8,
    pub remaining_top3: Vec<RemainingTaskItem>,
    pub pending_confirmations_count: usize,
    pub needs_confirmation: bool,
}

impl ProgressSnapshot {
    pub fn from_agent(agent_id: &str) -> Option<Self> {
        let team = extract_team_name(agent_id)?;
        Self::from_team(&team)
    }

    pub fn from_team(team: &str) -> Option<Self> {
        let bridge = TeamBridge::new();
        let tasks = bridge.read_team_tasks(team);
        let pending_confirmations = pending_confirmation_count(team);

        if tasks.is_empty() && pending_confirmations == 0 {
            return None;
        }

        Some(Self::from_parts(tasks, pending_confirmations))
    }

    pub fn from_parts(tasks: Vec<TeamTaskSummary>, pending_confirmations_count: usize) -> Self {
        let total_tasks = tasks.len();
        let completed_tasks = tasks
            .iter()
            .filter(|task| task.status == "completed")
            .count();
        let in_progress_tasks = tasks
            .iter()
            .filter(|task| task.status == "in_progress")
            .count();
        let pending_tasks = tasks.iter().filter(|task| task.status == "pending").count();
        let remaining_top3 = build_remaining_top3(&tasks);
        let completion_rate = if total_tasks == 0 {
            0
        } else {
            ((completed_tasks * 100) / total_tasks) as u8
        };

        Self {
            total_tasks,
            completed_tasks,
            pending_tasks,
            in_progress_tasks,
            completion_rate,
            remaining_top3,
            pending_confirmations_count,
            needs_confirmation: pending_confirmations_count > 0,
        }
    }
}

fn extract_team_name(agent_id: &str) -> Option<String> {
    agent_id
        .split_once('@')
        .map(|(_, team)| team.to_string())
        .filter(|team| !team.is_empty())
}

fn pending_confirmation_count(team: &str) -> usize {
    let manager = ConversationStateManager::new();
    manager
        .get_pending_confirmations()
        .map(|items| {
            items.into_iter()
                .filter(|item| item.team.as_deref() == Some(team))
                .count()
        })
        .unwrap_or(0)
}

fn build_remaining_top3(tasks: &[TeamTaskSummary]) -> Vec<RemainingTaskItem> {
    let mut remaining: Vec<_> = tasks
        .iter()
        .filter(|task| task.status != "completed")
        .collect();

    remaining.sort_by(|a, b| {
        task_rank(a)
            .cmp(&task_rank(b))
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.subject.cmp(&b.subject))
    });

    remaining
        .into_iter()
        .take(3)
        .map(|task| RemainingTaskItem {
            subject: task.subject.clone(),
            status: task.status.clone(),
            owner: task.owner.clone(),
        })
        .collect()
}

fn task_rank(task: &TeamTaskSummary) -> (u8, u8) {
    let dependency_rank = if task.blocked_by.is_empty() { 1 } else { 0 };
    let status_rank = match task.status.as_str() {
        "in_progress" => 0,
        "pending" => 1,
        _ => 2,
    };
    (dependency_rank, status_rank)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(
        id: &str,
        subject: &str,
        status: &str,
        owner: Option<&str>,
        blocked_by: &[&str],
    ) -> TeamTaskSummary {
        TeamTaskSummary {
            id: id.to_string(),
            subject: subject.to_string(),
            status: status.to_string(),
            owner: owner.map(|value| value.to_string()),
            blocked_by: blocked_by.iter().map(|value| value.to_string()).collect(),
        }
    }

    #[test]
    fn builds_progress_snapshot_counts() {
        let snapshot = ProgressSnapshot::from_parts(
            vec![
                make_task("1", "blocked", "pending", None, &["0"]),
                make_task("2", "doing", "in_progress", Some("alice"), &[]),
                make_task("3", "done", "completed", None, &[]),
            ],
            2,
        );

        assert_eq!(snapshot.total_tasks, 3);
        assert_eq!(snapshot.completed_tasks, 1);
        assert_eq!(snapshot.pending_tasks, 1);
        assert_eq!(snapshot.in_progress_tasks, 1);
        assert_eq!(snapshot.completion_rate, 33);
        assert_eq!(snapshot.pending_confirmations_count, 2);
        assert!(snapshot.needs_confirmation);
    }

    #[test]
    fn prioritizes_blocked_then_in_progress_then_pending() {
        let snapshot = ProgressSnapshot::from_parts(
            vec![
                make_task("3", "plain pending", "pending", None, &[]),
                make_task("2", "doing", "in_progress", Some("bob"), &[]),
                make_task("1", "blocked", "pending", None, &["x"]),
                make_task("4", "done", "completed", None, &[]),
            ],
            0,
        );

        let subjects: Vec<_> = snapshot
            .remaining_top3
            .iter()
            .map(|item| item.subject.as_str())
            .collect();
        assert_eq!(subjects, vec!["blocked", "doing", "plain pending"]);
    }

    #[test]
    fn returns_none_when_no_tasks_and_no_confirmations() {
        assert_eq!(ProgressSnapshot::from_parts(Vec::new(), 0).remaining_top3.len(), 0);
    }
}
