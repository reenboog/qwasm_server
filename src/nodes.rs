use crate::{encrypted::Encrypted, id::Uid, purge::Purge};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const NO_PARENT_ID: u64 = u64::MAX;
const ROOT_ID: u64 = 0;

#[derive(PartialEq, Debug)]
pub enum Error {
	NotFound(Uid),
	NotAllowed,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct LockedNode {
	pub id: Uid,
	pub parent_id: Uid,
	pub content: Encrypted,
	pub dirty: bool,
	// pending?
}

#[derive(Clone)]
pub struct Nodes {
	// keep a hash of the most recent state?
	// { parent_id, children_ids }
	branches: HashMap<Uid, Vec<Uid>>,
	// { id, node }
	nodes: HashMap<Uid, LockedNode>,
}

impl Nodes {
	pub fn add(&mut self, node: LockedNode) {
		let id = node.id;
		let parent = node.parent_id;

		self.nodes.insert(id, node);
		self.branches.entry(parent).or_default().push(id);
	}

	// returns ids of all the deleted nodes (the deleted one and its direct and indirect children)
	pub fn delete(&mut self, id: Uid) -> Vec<Uid> {
		let mut deleted = Vec::new();

		if let Some(node) = self.nodes.remove(&id) {
			deleted.push(id);

			if let Some(parent) = self.branches.get_mut(&node.parent_id) {
				parent.retain(|eid| *eid != id);
			}

			if let Some(children) = self.branches.remove(&id) {
				for child in children {
					deleted.extend(self.delete(child));
				}
			}
		}

		deleted
	}

	// having child ids in the specified list will affect order, but still won't allow duplicates
	pub fn delete_list(&mut self, ids: &[Uid]) -> Vec<Uid> {
		use std::collections::HashSet;

		let mut seen = HashSet::new();
		ids.iter()
			.flat_map(|id| self.delete(*id))
			.filter(|uid| seen.insert(*uid))
			.collect()
	}

	pub fn get_all(&self) -> Vec<LockedNode> {
		self.nodes.values().cloned().collect()
	}

	pub fn move_to(&mut self, id: Uid, new_parent: Uid) -> Result<(), Error> {
		// only one root is allowed
		if new_parent == NO_PARENT_ID {
			return Err(Error::NotAllowed);
		}

		let mut current = new_parent;
		// check to the top most node of the hierarchy: we always have a root whose parent is NO_PARENT_ID
		while current != Uid::new(NO_PARENT_ID) {
			if current == id {
				return Err(Error::NotAllowed);
			}

			if let Some(node) = self.nodes.get(&current) {
				current = node.parent_id;
			} else {
				return Err(Error::NotFound(new_parent));
			}
		}

		// Perform the move if the node exists
		if let Some(node) = self.nodes.get_mut(&id) {
			if node.parent_id == new_parent {
				Err(Error::NotAllowed)
			} else {
				// Remove id from its current parent's branches
				if let Some(parent) = self.branches.get_mut(&node.parent_id) {
					parent.retain(|eid| *eid != id);
				}

				// Update node's parent_id
				node.parent_id = new_parent;

				// Add id to the new parent's branches
				self.branches.entry(new_parent).or_default().push(id);

				Ok(())
			}
		} else {
			Err(Error::NotFound(id))
		}
	}
}

impl Purge for Nodes {
	fn new() -> Self {
		Self {
			branches: HashMap::new(),
			nodes: HashMap::new(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{encrypted::Encrypted, salt::Salt};

	fn stub_encrypted() -> Encrypted {
		Encrypted {
			ct: vec![],
			salt: Salt::generate(),
		}
	}

	#[test]
	fn test_move_node_to_itself() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(
			storage.move_to(Uid::new(0), Uid::new(0)),
			Err(Error::NotAllowed)
		);
	}

	#[test]
	fn test_move_node_to_own_parent() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(
			storage.move_to(Uid::new(1), Uid::new(0)),
			Err(Error::NotAllowed)
		);
	}

	#[test]
	fn test_move_node_to_non_existent_parent() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(
			storage.move_to(Uid::new(1), Uid::new(999)),
			Err(Error::NotFound(Uid::new(999)))
		);
	}

	#[test]
	fn test_move_non_existent_node() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(
			storage.move_to(Uid::new(999), Uid::new(0)),
			Err(Error::NotFound(Uid::new(999)))
		);
	}

	#[test]
	fn test_move_node_to_valid_parent() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});

		storage.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		storage.add(LockedNode {
			id: Uid::new(2),
			parent_id: Uid::new(1),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(storage.move_to(Uid::new(2), Uid::new(0)), Ok(()));
	}

	#[test]
	fn test_move_node_outside_hierarchy() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(
			storage.move_to(Uid::new(0), Uid::new(NO_PARENT_ID)),
			Err(Error::NotAllowed)
		);
		assert_eq!(
			storage.move_to(Uid::new(1), Uid::new(NO_PARENT_ID)),
			Err(Error::NotAllowed)
		);
	}

	#[test]
	fn test_prevent_circular_reference() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(2),
			parent_id: Uid::new(1),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(3),
			parent_id: Uid::new(2),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(
			storage.move_to(Uid::new(0), Uid::new(1)),
			Err(Error::NotAllowed)
		);
		assert_eq!(
			storage.move_to(Uid::new(0), Uid::new(2)),
			Err(Error::NotAllowed)
		);
		assert_eq!(
			storage.move_to(Uid::new(0), Uid::new(3)),
			Err(Error::NotAllowed)
		);
		assert_eq!(
			storage.move_to(Uid::new(1), Uid::new(2)),
			Err(Error::NotAllowed)
		);
		assert_eq!(
			storage.move_to(Uid::new(1), Uid::new(3)),
			Err(Error::NotAllowed)
		);
	}

	#[test]
	fn test_move_node_several_times() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(2),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(3),
			parent_id: Uid::new(1),
			content: stub_encrypted(),
			dirty: false,
		});

		// 0
		//  1
		//   3
		//  2
		assert_eq!(storage.move_to(Uid::new(3), Uid::new(2)), Ok(()));
		assert_eq!(storage.move_to(Uid::new(3), Uid::new(1)), Ok(()));
		assert_eq!(storage.move_to(Uid::new(2), Uid::new(3)), Ok(()));
		assert_eq!(storage.move_to(Uid::new(2), Uid::new(1)), Ok(()));
		assert_eq!(storage.move_to(Uid::new(3), Uid::new(0)), Ok(()));
		assert_eq!(storage.move_to(Uid::new(2), Uid::new(0)), Ok(()));

		assert_eq!(storage.branches.get(&Uid::new(0)).unwrap().len(), 3);
	}

	#[test]
	fn test_delete_node_no_children() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(storage.nodes.contains_key(&Uid::new(0)), true);
		storage.delete(Uid::new(0));
		assert_eq!(storage.nodes.contains_key(&Uid::new(0)), false);
	}

	#[test]
	fn test_delete_node_with_children() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(2),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(storage.nodes.contains_key(&Uid::new(0)), true);
		assert_eq!(storage.nodes.contains_key(&Uid::new(1)), true);
		assert_eq!(storage.nodes.contains_key(&Uid::new(2)), true);

		storage.delete(Uid::new(0));

		assert_eq!(storage.nodes.contains_key(&Uid::new(0)), false);
		assert_eq!(storage.nodes.contains_key(&Uid::new(1)), false);
		assert_eq!(storage.nodes.contains_key(&Uid::new(2)), false);
	}

	#[test]
	fn test_delete_non_existent_node() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(storage.nodes.contains_key(&Uid::new(0)), true);
		storage.delete(Uid::new(999)); // Trying to remove a non-existent node
		assert_eq!(storage.nodes.contains_key(&Uid::new(0)), true);
	}

	#[test]
	fn test_delete_root_node() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(storage.nodes.contains_key(&Uid::new(0)), true);
		assert_eq!(storage.nodes.contains_key(&Uid::new(1)), true);

		storage.delete(Uid::new(0));

		assert_eq!(storage.nodes.contains_key(&Uid::new(0)), false);
		assert_eq!(storage.nodes.contains_key(&Uid::new(1)), false);
	}

	#[test]
	fn test_delete_leaf_node() {
		let mut storage = Nodes::new();

		storage.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});
		storage.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(storage.nodes.contains_key(&Uid::new(1)), true);
		storage.delete(Uid::new(1));
		assert_eq!(storage.nodes.contains_key(&Uid::new(1)), false);
		assert!(storage.branches.get(&Uid::new(0)).unwrap().is_empty());
	}

	#[test]
	fn test_delete_returns_whole_subtree_in_one_root() {
		let mut one_root = Nodes::new();

		/*

		0
			1
				11
					111
				12
					121
			2
				21
			3

		*/

		// 0
		one_root.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});

		// 1
		one_root.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});
		// 11
		one_root.add(LockedNode {
			id: Uid::new(11),
			parent_id: Uid::new(1),
			content: stub_encrypted(),
			dirty: false,
		});
		// 111
		one_root.add(LockedNode {
			id: Uid::new(111),
			parent_id: Uid::new(11),
			content: stub_encrypted(),
			dirty: false,
		});
		// 12
		one_root.add(LockedNode {
			id: Uid::new(12),
			parent_id: Uid::new(1),
			content: stub_encrypted(),
			dirty: false,
		});
		// 121
		one_root.add(LockedNode {
			id: Uid::new(121),
			parent_id: Uid::new(12),
			content: stub_encrypted(),
			dirty: false,
		});
		// 2
		one_root.add(LockedNode {
			id: Uid::new(2),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});
		// 21
		one_root.add(LockedNode {
			id: Uid::new(21),
			parent_id: Uid::new(2),
			content: stub_encrypted(),
			dirty: false,
		});
		// 3
		one_root.add(LockedNode {
			id: Uid::new(3),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(
			one_root.clone().delete(Uid::new(0)),
			[0, 1, 11, 111, 12, 121, 2, 21, 3]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(1)),
			[1, 11, 111, 12, 121]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(11)),
			[11, 111]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(111)),
			[111]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(12)),
			[12, 121]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(121)),
			[121]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(2)),
			[2, 21]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(21)),
			[21].into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(3)),
			[3].into_iter().map(|idx| Uid::new(idx)).collect::<Vec<_>>()
		);
		// empty list for a non existing node
		assert_eq!(
			one_root.clone().delete(Uid::new(999)),
			[].into_iter().map(|idx| Uid::new(idx)).collect::<Vec<_>>()
		);
	}

	#[test]
	fn test_delete_returns_whole_subtree_in_when_detached() {
		let mut one_root = Nodes::new();

		/*

		0?
			1?
				11
					111
				12
					121
			2
				21
			3

		*/

		// 11, orphant
		one_root.add(LockedNode {
			id: Uid::new(11),
			parent_id: Uid::new(1),
			content: stub_encrypted(),
			dirty: false,
		});
		// 111
		one_root.add(LockedNode {
			id: Uid::new(111),
			parent_id: Uid::new(11),
			content: stub_encrypted(),
			dirty: false,
		});
		// 12, orphant
		one_root.add(LockedNode {
			id: Uid::new(12),
			parent_id: Uid::new(1),
			content: stub_encrypted(),
			dirty: false,
		});
		// 121
		one_root.add(LockedNode {
			id: Uid::new(121),
			parent_id: Uid::new(12),
			content: stub_encrypted(),
			dirty: false,
		});
		// 2, orphant
		one_root.add(LockedNode {
			id: Uid::new(2),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});
		// 21
		one_root.add(LockedNode {
			id: Uid::new(21),
			parent_id: Uid::new(2),
			content: stub_encrypted(),
			dirty: false,
		});
		// 3, orphant
		one_root.add(LockedNode {
			id: Uid::new(3),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(
			one_root.clone().delete(Uid::new(11)),
			[11, 111]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(111)),
			[111]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(12)),
			[12, 121]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(121)),
			[121]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(2)),
			[2, 21]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(21)),
			[21].into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(3)),
			[3].into_iter().map(|idx| Uid::new(idx)).collect::<Vec<_>>()
		);
		// empty list for a non existing node
		assert_eq!(
			one_root.clone().delete(Uid::new(0)),
			[].into_iter().map(|idx| Uid::new(idx)).collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(1)),
			[].into_iter().map(|idx| Uid::new(idx)).collect::<Vec<_>>()
		);
		assert_eq!(
			one_root.clone().delete(Uid::new(999)),
			[].into_iter().map(|idx| Uid::new(idx)).collect::<Vec<_>>()
		);
	}

	#[test]
	fn test_rdelete_list() {
		let mut one_root = Nodes::new();

		/*

		0
			1
				11
					111
				12
					121
			2
				21
			3

		*/

		// 0
		one_root.add(LockedNode {
			id: Uid::new(0),
			parent_id: Uid::new(NO_PARENT_ID),
			content: stub_encrypted(),
			dirty: false,
		});
		// 1
		one_root.add(LockedNode {
			id: Uid::new(1),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});
		// 11
		one_root.add(LockedNode {
			id: Uid::new(11),
			parent_id: Uid::new(1),
			content: stub_encrypted(),
			dirty: false,
		});
		// 111
		one_root.add(LockedNode {
			id: Uid::new(111),
			parent_id: Uid::new(11),
			content: stub_encrypted(),
			dirty: false,
		});
		// 12
		one_root.add(LockedNode {
			id: Uid::new(12),
			parent_id: Uid::new(1),
			content: stub_encrypted(),
			dirty: false,
		});
		// 121
		one_root.add(LockedNode {
			id: Uid::new(121),
			parent_id: Uid::new(12),
			content: stub_encrypted(),
			dirty: false,
		});
		// 2, orphant
		one_root.add(LockedNode {
			id: Uid::new(2),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});
		// 21
		one_root.add(LockedNode {
			id: Uid::new(21),
			parent_id: Uid::new(2),
			content: stub_encrypted(),
			dirty: false,
		});
		// 3, orphant
		one_root.add(LockedNode {
			id: Uid::new(3),
			parent_id: Uid::new(0),
			content: stub_encrypted(),
			dirty: false,
		});

		assert_eq!(
			// specifying children won't do harm
			one_root
				.clone()
				.delete_list(&[Uid::new(0), Uid::new(111), Uid::new(12), Uid::new(3)]),
			[0, 1, 11, 111, 12, 121, 2, 21, 3]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);

		assert_eq!(
			// non existing ids won't do harm either
			one_root
				.clone()
				.delete_list(&[Uid::new(11), Uid::new(111), Uid::new(999)]),
			[11, 111]
				.into_iter()
				.map(|idx| Uid::new(idx))
				.collect::<Vec<_>>()
		);
	}
}
