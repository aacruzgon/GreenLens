//! The role matrix ([contracts 1], roles and access) as data: every action and the lowest role
//! that may do it. Compile-time checks in handlers use [`crate::authz`]; this table is what
//! those markers and the agent's tool filter are tested against.

use crate::wire::Role;

/// Every action of the role matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Read gardens, plants, scans, plans, tasks, timeline, measurements, the garden map.
    Read,
    /// Chat in their own conversations, with the read tools and `remember`.
    Chat,
    /// Scan feedback.
    GiveFeedback,
    /// Create scans, upload, delete a scan before it is submitted, answer questions, confirm
    /// species, add guided soil checks.
    Scan,
    /// Map the garden (from a `LiDAR` device).
    MapGarden,
    /// Chat write tools (create and complete tasks, revise plans) and their cards.
    UseWriteTools,
    /// Create, complete and dismiss tasks.
    ManageTasks,
    /// Create and edit plants.
    EditPlants,
    /// Share their own chat memories with the garden, add garden notes.
    ShareMemories,
    /// Rename the garden, manage invites and members, change roles, delete plants or the garden,
    /// delete shared memories.
    ManageGarden,
}

impl Action {
    /// Every action.
    pub const ALL: [Self; 10] = [
        Self::Read,
        Self::Chat,
        Self::GiveFeedback,
        Self::Scan,
        Self::MapGarden,
        Self::UseWriteTools,
        Self::ManageTasks,
        Self::EditPlants,
        Self::ShareMemories,
        Self::ManageGarden,
    ];

    /// The lowest role allowed: the matrix of contracts 1.
    pub const fn min_role(self) -> Role {
        match self {
            Self::Read | Self::Chat => Role::Viewer,
            Self::GiveFeedback => Role::Reviewer,
            Self::Scan
            | Self::MapGarden
            | Self::UseWriteTools
            | Self::ManageTasks
            | Self::EditPlants
            | Self::ShareMemories => Role::Editor,
            Self::ManageGarden => Role::Owner,
        }
    }
}

impl Role {
    /// Whether this role may do `action`: ranks are ordered, so "editor or above".
    pub fn allows(self, action: Action) -> bool {
        self >= action.min_role()
    }
}

/// A role change that would leave the garden without an owner: the last owner can't leave or
/// be demoted (`409 invalid_state`).
pub fn leaves_no_owner(owners: u32, member_role: Role, new_role: Option<Role>) -> bool {
    member_role == Role::Owner && owners <= 1 && new_role != Some(Role::Owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_matrix() {
        let allowed = |role: Role| -> Vec<Action> {
            Action::ALL
                .into_iter()
                .filter(|a| role.allows(*a))
                .collect()
        };
        assert_eq!(allowed(Role::Viewer), vec![Action::Read, Action::Chat]);
        assert_eq!(
            allowed(Role::Reviewer),
            vec![Action::Read, Action::Chat, Action::GiveFeedback]
        );
        assert_eq!(allowed(Role::Editor).len(), 9);
        assert!(!Role::Editor.allows(Action::ManageGarden));
        assert_eq!(allowed(Role::Owner).len(), 10);
    }

    #[test]
    fn last_owner() {
        assert!(leaves_no_owner(1, Role::Owner, None));
        assert!(leaves_no_owner(1, Role::Owner, Some(Role::Editor)));
        assert!(!leaves_no_owner(2, Role::Owner, None));
        assert!(!leaves_no_owner(1, Role::Editor, None));
    }
}
