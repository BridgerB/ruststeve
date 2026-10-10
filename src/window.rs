//! Inventory windows — player inventory, chests, furnaces, etc. Slot layout,
//! item search/fill/dump, and client-side click simulation. Port of typecraft's
//! `window` module.

use std::collections::HashMap;

use crate::item::{create_item, items_equal, Item};
use crate::nbt::{equal_nbt, NbtCompound, NbtTag};
use crate::registry::Registry;

/// A click action on a window slot (mirrors the Click Window packet).
#[derive(Debug, Clone, Copy)]
pub struct Click {
    pub mode: i32,
    pub mouse_button: i32,
    pub slot: i32,
}

/// Slot layout for a window kind.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub key: String,
    pub inventory_start: usize,
    pub inventory_end_inclusive: usize,
    pub slots: usize,
    pub craft: i32,
    pub require_confirmation: bool,
    pub type_id: i64,
}

/// A Minecraft inventory window.
#[derive(Debug, Clone)]
pub struct Window {
    pub id: i32,
    pub kind: String,
    pub title: String,
    pub slots: Vec<Option<Item>>,
    pub inventory_start: usize,
    pub inventory_end: usize,
    pub hotbar_start: usize,
    pub crafting_result_slot: i32,
    pub requires_confirmation: bool,
    pub selected_item: Option<Item>,
    /// Server-sent state id, echoed back in `container_click` (1.17+).
    pub state_id: i32,
}

fn with_count(registry: &Registry, item: &Item, count: i32) -> Item {
    create_item(
        registry,
        item.type_id,
        count,
        item.metadata,
        item.nbt.clone(),
        item.components.clone(),
        item.removed_components.clone(),
    )
}

fn nbt_matches(query: Option<&NbtCompound>, item_nbt: Option<&NbtCompound>) -> bool {
    match query {
        None => true,
        Some(q) => item_nbt
            .map(|n| equal_nbt(&NbtTag::Compound(q.clone()), &NbtTag::Compound(n.clone())))
            .unwrap_or(false),
    }
}

/// Where a slot of one window shows up in another: the player's 36 main + hotbar slots appear in the
/// player window (id 0, slots 9–44) AND in every open container's inventory section.
fn mirror_slot(from: &Window, to: &Window, slot: usize) -> Option<usize> {
    if slot < from.inventory_start || slot >= from.inventory_end {
        return None;
    }
    let t = to.inventory_start + (slot - from.inventory_start);
    (t < to.inventory_end && t < to.slots.len()).then_some(t)
}

/// Apply a server `container_set_slot` verbatim (cycle 6, decision 7). The update lands in its window and
/// its mirror in the other one, so the player window and an open container never disagree about the
/// player's own slots: closing a container no longer copies a stale view over newer updates.
pub fn apply_server_slot(inv: &mut Window, container: Option<&mut Window>, window_id: i32, slot: i32, item: Option<Item>) {
    let Ok(slot) = usize::try_from(slot) else { return };
    if window_id == 0 || window_id == -1 {
        if let Some(c) = container {
            if let Some(t) = mirror_slot(inv, c, slot) {
                c.slots[t] = item.clone();
            }
        }
        if slot < inv.slots.len() {
            inv.slots[slot] = item;
        }
    } else if let Some(c) = container.filter(|c| c.id == window_id) {
        if let Some(t) = mirror_slot(c, inv, slot) {
            inv.slots[t] = item.clone();
        }
        if slot < c.slots.len() {
            c.slots[slot] = item;
        }
    }
}

/// Apply a server `container_set_content` verbatim, slot by slot (see `apply_server_slot`). The server's
/// content is the truth, including items it holds in the 2×2 crafting grid.
pub fn apply_server_content(inv: &mut Window, mut container: Option<&mut Window>, window_id: i32, items: Vec<Option<Item>>) {
    for (i, item) in items.into_iter().enumerate() {
        apply_server_slot(inv, container.as_deref_mut(), window_id, i as i32, item);
    }
}

impl Window {
    pub fn new(
        id: i32,
        kind: impl Into<String>,
        title: impl Into<String>,
        slot_count: usize,
        inventory_start: usize,
        inventory_end_inclusive: usize,
        crafting_result_slot: i32,
        requires_confirmation: bool,
    ) -> Window {
        let inventory_end = inventory_end_inclusive + 1;
        Window {
            id,
            kind: kind.into(),
            title: title.into(),
            slots: vec![None; slot_count],
            inventory_start,
            inventory_end,
            hotbar_start: inventory_end - 9,
            crafting_result_slot,
            requires_confirmation,
            selected_item: None,
            state_id: 0,
        }
    }

    fn slot(&self, i: i32) -> Option<&Item> {
        if i < 0 {
            return None;
        }
        self.slots.get(i as usize).and_then(|s| s.as_ref())
    }

    pub fn update_slot(&mut self, slot: usize, new_item: Option<Item>) {
        if slot < self.slots.len() {
            self.slots[slot] = new_item;
        }
    }

    // ── Fill / dump ──

    pub fn fill_and_dump(
        &mut self,
        registry: &Registry,
        source: usize,
        start: usize,
        end: usize,
        last_to_first: bool,
    ) {
        let (ty, meta, nbt) = match &self.slots[source] {
            Some(it) => (it.type_id, it.metadata, it.nbt.clone()),
            None => return,
        };
        let matching = self.find_items_range(start, end, ty, Some(meta), true, nbt.as_ref(), true);
        self.fill_slots_with_item(registry, matching, source, last_to_first);
        if self.slots[source].is_some() {
            self.dump_item(source, start, end, last_to_first);
        }
    }

    pub fn fill_slots_with_item(
        &mut self,
        registry: &Registry,
        mut targets: Vec<usize>,
        source: usize,
        last_to_first: bool,
    ) {
        while !targets.is_empty() && self.slots[source].is_some() {
            let target = if last_to_first {
                targets.pop().unwrap()
            } else {
                targets.remove(0)
            };
            self.fill_slot_with_item(registry, target, source);
        }
    }

    pub fn fill_slot_with_item(&mut self, registry: &Registry, fill_slot: usize, take_slot: usize) {
        let (fill, take) = match (&self.slots[fill_slot], &self.slots[take_slot]) {
            (Some(f), Some(t)) => (f.clone(), t.clone()),
            _ => return,
        };
        let new_count = fill.count + take.count;
        let leftover = new_count - fill.stack_size;
        if leftover <= 0 {
            self.update_slot(fill_slot, Some(with_count(registry, &fill, new_count)));
            self.update_slot(take_slot, None);
        } else {
            self.update_slot(
                fill_slot,
                Some(with_count(registry, &fill, fill.stack_size)),
            );
            self.update_slot(take_slot, Some(with_count(registry, &take, leftover)));
        }
    }

    pub fn fill_slot_with_selected_item(
        &mut self,
        registry: &Registry,
        slot: usize,
        until_full: bool,
    ) {
        let (item, selected) = match (&self.slots[slot], &self.selected_item) {
            (Some(i), Some(s)) => (i.clone(), s.clone()),
            _ => return,
        };
        if until_full {
            let new_count = item.count + selected.count;
            let leftover = new_count - item.stack_size;
            if leftover <= 0 {
                self.update_slot(slot, Some(with_count(registry, &item, new_count)));
                self.selected_item = None;
            } else {
                self.update_slot(slot, Some(with_count(registry, &item, item.stack_size)));
                self.selected_item = Some(with_count(registry, &selected, leftover));
            }
        } else if item.count + 1 <= item.stack_size {
            self.update_slot(slot, Some(with_count(registry, &item, item.count + 1)));
            self.selected_item = if selected.count - 1 == 0 {
                None
            } else {
                Some(with_count(registry, &selected, selected.count - 1))
            };
        }
    }

    pub fn dump_item(&mut self, source: usize, start: usize, end: usize, last_to_first: bool) {
        let empty = if last_to_first {
            self.last_empty_slot_range(start, end)
        } else {
            self.first_empty_slot_range(start, end)
        };
        if let Some(empty) = empty {
            if empty as i32 != self.crafting_result_slot {
                let item = self.slots[source].clone();
                self.update_slot(empty, item);
                self.update_slot(source, None);
            }
        }
    }

    pub fn split_slot(&mut self, registry: &Registry, slot: usize) {
        let Some(item) = self.slots[slot].clone() else {
            return;
        };
        let cursor_count = (item.count + 1) / 2; // ceil for positive counts
        self.selected_item = Some(with_count(registry, &item, cursor_count));
        let remaining = item.count - cursor_count;
        if remaining == 0 {
            self.update_slot(slot, None);
        } else {
            self.update_slot(slot, Some(with_count(registry, &item, remaining)));
        }
    }

    pub fn swap_selected_item(&mut self, slot: usize) {
        let item = self.slots[slot].take();
        self.slots[slot] = self.selected_item.take();
        self.selected_item = item;
    }

    pub fn drop_selected_item(&mut self, until_empty: bool) {
        match &self.selected_item {
            Some(sel) if !until_empty && sel.count - 1 != 0 => {
                let mut s = sel.clone();
                s.count -= 1;
                self.selected_item = Some(s);
            }
            _ => self.selected_item = None,
        }
    }

    // ── Click handling ──

    pub fn accept_click(&mut self, registry: &Registry, click: Click, gamemode: i32) -> Vec<i32> {
        match click.mode {
            0 => self.mouse_click(registry, click),
            1 => {
                self.shift_click(registry, click);
                vec![]
            }
            2 => {
                self.number_click(registry, click);
                vec![]
            }
            3 => self.middle_click(registry, click, gamemode),
            4 => self.drop_click(registry, click),
            _ => vec![],
        }
    }

    pub fn mouse_click(&mut self, registry: &Registry, click: Click) -> Vec<i32> {
        if click.slot == -999 {
            self.drop_selected_item(click.mouse_button == 0);
            return vec![];
        }
        let slot = click.slot as usize;
        let item = self.slot(click.slot).cloned();

        if click.mouse_button == 0 {
            if let (Some(item), Some(selected)) = (item.clone(), self.selected_item.clone()) {
                if items_equal(Some(&item), Some(&selected), false, true) {
                    if click.slot == self.crafting_result_slot {
                        let max_transfer = selected.stack_size - selected.count;
                        if item.count > max_transfer {
                            self.selected_item = Some(with_count(
                                registry,
                                &selected,
                                selected.count + max_transfer,
                            ));
                            self.update_slot(
                                slot,
                                Some(with_count(registry, &item, item.count - max_transfer)),
                            );
                        } else {
                            self.selected_item =
                                Some(with_count(registry, &selected, selected.count + item.count));
                            self.update_slot(slot, None);
                        }
                    } else {
                        self.fill_slot_with_selected_item(registry, slot, true);
                    }
                } else {
                    self.swap_selected_item(slot);
                }
                return vec![click.slot];
            }
            if self.selected_item.is_some() || item.is_some() {
                self.swap_selected_item(slot);
                return vec![click.slot];
            }
        } else if click.mouse_button == 1 {
            if let Some(selected) = self.selected_item.clone() {
                if let Some(item) = item {
                    if items_equal(Some(&item), Some(&selected), false, true) {
                        self.fill_slot_with_selected_item(registry, slot, false);
                    } else {
                        self.swap_selected_item(slot);
                    }
                } else {
                    let new_item = with_count(registry, &selected, 0);
                    self.update_slot(slot, Some(new_item));
                    self.fill_slot_with_selected_item(registry, slot, false);
                }
                return vec![click.slot];
            }
            if item.is_some() && click.slot != self.crafting_result_slot {
                self.split_slot(registry, slot);
                return vec![click.slot];
            }
        }
        vec![]
    }

    pub fn shift_click(&mut self, registry: &Registry, click: Click) {
        if self.slot(click.slot).is_none() {
            return;
        }
        let inv_start = self.inventory_start;
        let inv_end = self.inventory_end;
        let hotbar = self.hotbar_start;
        if self.kind == "minecraft:inventory" {
            if (click.slot as usize) < inv_start {
                self.fill_and_dump(
                    registry,
                    click.slot as usize,
                    inv_start,
                    inv_end,
                    click.slot == self.crafting_result_slot,
                );
            } else if (click.slot as usize) < inv_end - 10 {
                self.fill_and_dump(registry, click.slot as usize, hotbar, inv_end, false);
            } else {
                self.fill_and_dump(registry, click.slot as usize, inv_start, inv_end, false);
            }
        } else if (click.slot as usize) < inv_start {
            let last = self.crafting_result_slot == -1 || click.slot == self.crafting_result_slot;
            self.fill_and_dump(registry, click.slot as usize, inv_start, inv_end, last);
        } else {
            self.fill_and_dump(
                registry,
                click.slot as usize,
                0,
                inv_start.saturating_sub(1),
                false,
            );
        }
    }

    pub fn number_click(&mut self, registry: &Registry, click: Click) {
        if self.selected_item.is_some() {
            return;
        }
        let item = self.slot(click.slot).cloned();
        let hotbar_slot = self.hotbar_start + click.mouse_button as usize;
        let item_at_hotbar = self.slots.get(hotbar_slot).and_then(|s| s.clone());

        if items_equal(item.as_ref(), item_at_hotbar.as_ref(), true, true)
            && item.is_some()
            && click.slot as usize == hotbar_slot
        {
            return;
        }

        if let Some(item) = item {
            if let Some(hotbar_item) = item_at_hotbar.clone() {
                if (self.kind == "minecraft:inventory" || registry.is_newer_or_equal_to("1.9"))
                    && click.slot != self.crafting_result_slot
                {
                    self.update_slot(click.slot as usize, Some(hotbar_item));
                    self.update_slot(hotbar_slot, Some(item));
                } else {
                    self.dump_item(hotbar_slot, self.hotbar_start, self.inventory_end, false);
                    if self.slots[hotbar_slot].is_some() {
                        self.dump_item(
                            hotbar_slot,
                            self.inventory_start,
                            self.hotbar_start.saturating_sub(1),
                            false,
                        );
                    }
                    if self.slots[hotbar_slot].is_none() {
                        self.update_slot(click.slot as usize, None);
                        self.update_slot(hotbar_slot, Some(item));
                        let mut slots = self.find_items_range(
                            self.hotbar_start,
                            self.inventory_end,
                            hotbar_item.type_id,
                            Some(hotbar_item.metadata),
                            true,
                            hotbar_item.nbt.as_ref(),
                            false,
                        );
                        slots.extend(self.find_items_range(
                            self.inventory_start,
                            self.hotbar_start.saturating_sub(1),
                            hotbar_item.type_id,
                            Some(hotbar_item.metadata),
                            true,
                            hotbar_item.nbt.as_ref(),
                            false,
                        ));
                        if let Some(dumped) = self.find_item_range(
                            0,
                            self.inventory_end,
                            hotbar_item.type_id,
                            Some(hotbar_item.metadata),
                            false,
                            hotbar_item.nbt.as_ref(),
                            false,
                        ) {
                            slots.retain(|&s| s != dumped);
                            self.fill_slots_with_item(registry, slots, dumped, false);
                        }
                    }
                }
            } else {
                self.update_slot(click.slot as usize, None);
                self.update_slot(hotbar_slot, Some(item));
            }
        } else if item_at_hotbar.is_some() && click.slot != self.crafting_result_slot {
            self.update_slot(click.slot as usize, item_at_hotbar);
            self.update_slot(hotbar_slot, None);
        }
    }

    pub fn middle_click(&mut self, registry: &Registry, click: Click, gamemode: i32) -> Vec<i32> {
        if self.selected_item.is_some() {
            return vec![];
        }
        if gamemode == 1 {
            if let Some(item) = self.slot(click.slot).cloned() {
                self.selected_item = Some(with_count(registry, &item, item.stack_size));
            }
        }
        vec![]
    }

    pub fn drop_click(&mut self, registry: &Registry, click: Click) -> Vec<i32> {
        let item = self.slot(click.slot).cloned();
        if self.selected_item.is_some() || item.is_none() {
            return vec![];
        }
        let item = item.unwrap();
        let slot = click.slot as usize;
        if click.mouse_button == 0 {
            if item.count - 1 == 0 {
                self.update_slot(slot, None);
            } else {
                self.update_slot(slot, Some(with_count(registry, &item, item.count - 1)));
            }
            return vec![click.slot];
        }
        if click.mouse_button == 1 {
            self.update_slot(slot, None);
            return vec![click.slot];
        }
        vec![]
    }

    // ── Search ──

    #[allow(clippy::too_many_arguments)]
    /// First slot index in `start..end` whose (present) item satisfies `pred(index,
    /// item)`. The slot-walk shared by all the typed finders below.
    fn find_slot_in(
        &self,
        start: usize,
        end: usize,
        pred: impl Fn(usize, &Item) -> bool,
    ) -> Option<usize> {
        (start..end).find(|&i| {
            self.slots.get(i).and_then(|s| s.as_ref()).is_some_and(|item| pred(i, item))
        })
    }

    pub fn find_item_range(
        &self,
        start: usize,
        end: usize,
        item_type: i32,
        metadata: Option<i32>,
        not_full: bool,
        nbt: Option<&NbtCompound>,
        skip_craft_result: bool,
    ) -> Option<usize> {
        self.find_slot_in(start, end, |i, item| {
            item_type == item.type_id
                && metadata.map(|m| m == item.metadata).unwrap_or(true)
                && (!not_full || item.count < item.stack_size)
                && nbt_matches(nbt, item.nbt.as_ref())
                && !(i as i32 == self.crafting_result_slot && skip_craft_result)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn find_items_range(
        &self,
        start: usize,
        end: usize,
        item_type: i32,
        metadata: Option<i32>,
        not_full: bool,
        nbt: Option<&NbtCompound>,
        skip_craft_result: bool,
    ) -> Vec<usize> {
        let mut result = Vec::new();
        let mut pos = start;
        while pos < end {
            match self.find_item_range(
                pos,
                end,
                item_type,
                metadata,
                not_full,
                nbt,
                skip_craft_result,
            ) {
                Some(found) => {
                    result.push(found);
                    pos = found + 1;
                }
                None => break,
            }
        }
        result
    }

    pub fn find_item_range_name(
        &self,
        start: usize,
        end: usize,
        name: &str,
        metadata: Option<i32>,
        not_full: bool,
    ) -> Option<usize> {
        self.find_slot_in(start, end, |_, item| {
            item.name == name
                && metadata.map(|m| m == item.metadata).unwrap_or(true)
                && (!not_full || item.count < item.stack_size)
        })
    }

    pub fn find_inventory_item(
        &self,
        name: &str,
        metadata: Option<i32>,
        not_full: bool,
    ) -> Option<usize> {
        self.find_item_range_name(
            self.inventory_start,
            self.inventory_end,
            name,
            metadata,
            not_full,
        )
    }

    pub fn find_container_item(
        &self,
        name: &str,
        metadata: Option<i32>,
        not_full: bool,
    ) -> Option<usize> {
        self.find_item_range_name(0, self.inventory_start, name, metadata, not_full)
    }

    pub fn first_empty_slot_range(&self, start: usize, end: usize) -> Option<usize> {
        (start..end).find(|&i| self.slots.get(i).map(|s| s.is_none()).unwrap_or(false))
    }

    pub fn last_empty_slot_range(&self, start: usize, end: usize) -> Option<usize> {
        (start..=end)
            .rev()
            .find(|&i| self.slots.get(i).map(|s| s.is_none()).unwrap_or(false))
    }

    pub fn first_empty_hotbar_slot(&self) -> Option<usize> {
        self.first_empty_slot_range(self.hotbar_start, self.inventory_end)
    }

    pub fn first_empty_inventory_slot(&self, hotbar_first: bool) -> Option<usize> {
        if hotbar_first {
            if let Some(s) = self.first_empty_hotbar_slot() {
                return Some(s);
            }
        }
        self.first_empty_slot_range(self.inventory_start, self.inventory_end)
    }

    // ── Counting & listing ──

    pub fn count_range(
        &self,
        start: usize,
        end: usize,
        item_type: i32,
        metadata: Option<i32>,
    ) -> i32 {
        let mut sum = 0;
        for i in start..end {
            if let Some(item) = self.slots.get(i).and_then(|s| s.as_ref()) {
                if item_type == item.type_id && metadata.map(|m| m == item.metadata).unwrap_or(true)
                {
                    sum += item.count;
                }
            }
        }
        sum
    }

    pub fn window_count(&self, item_type: i32, metadata: Option<i32>) -> i32 {
        self.count_range(
            self.inventory_start,
            self.inventory_end,
            item_type,
            metadata,
        )
    }

    pub fn container_count(&self, item_type: i32, metadata: Option<i32>) -> i32 {
        self.count_range(0, self.inventory_start, item_type, metadata)
    }

    pub fn items_range(&self, start: usize, end: usize) -> Vec<&Item> {
        (start..end)
            .filter_map(|i| self.slots.get(i).and_then(|s| s.as_ref()))
            .collect()
    }

    pub fn window_items(&self) -> Vec<&Item> {
        self.items_range(self.inventory_start, self.inventory_end)
    }

    pub fn container_items(&self) -> Vec<&Item> {
        self.items_range(0, self.inventory_start)
    }

    pub fn empty_slot_count(&self) -> usize {
        (self.inventory_start..self.inventory_end)
            .filter(|&i| self.slots.get(i).map(|s| s.is_none()).unwrap_or(false))
            .count()
    }

    pub fn requires_confirmation(&self) -> bool {
        self.requires_confirmation
    }
}

// ── Window type table ──

/// Build the version-specific window type registry.
pub fn window_types(registry: &Registry) -> HashMap<String, WindowInfo> {
    let mut windows = HashMap::new();
    if registry
        .support_feature("village&pillageInventoryWindows")
        .as_bool()
    {
        let mut protocol_id: i64 = -1;
        let mut add = |key: &str,
                       start: usize,
                       end: usize,
                       slots: usize,
                       craft: i32,
                       confirm: bool,
                       windows: &mut HashMap<String, WindowInfo>| {
            windows.insert(
                key.to_string(),
                WindowInfo {
                    key: key.to_string(),
                    inventory_start: start,
                    inventory_end_inclusive: end,
                    slots,
                    craft,
                    require_confirmation: confirm,
                    type_id: protocol_id,
                },
            );
            protocol_id += 1;
        };
        add("minecraft:inventory", 9, 44, 46, 0, true, &mut windows);
        add("minecraft:generic_9x1", 9, 44, 45, -1, true, &mut windows);
        add("minecraft:generic_9x2", 18, 53, 54, -1, true, &mut windows);
        add("minecraft:generic_9x3", 27, 62, 63, -1, true, &mut windows);
        add("minecraft:generic_9x4", 36, 71, 72, -1, true, &mut windows);
        add("minecraft:generic_9x5", 45, 80, 81, -1, true, &mut windows);
        add("minecraft:generic_9x6", 54, 89, 90, -1, true, &mut windows);
        add("minecraft:generic_3x3", 9, 44, 45, -1, true, &mut windows);
        if registry.is_newer_or_equal_to("1.20.3") {
            add("minecraft:crafter_3x3", 10, 45, 46, -1, true, &mut windows);
        }
        add("minecraft:anvil", 3, 38, 39, 2, true, &mut windows);
        add("minecraft:beacon", 1, 36, 37, -1, true, &mut windows);
        add("minecraft:blast_furnace", 3, 38, 39, 2, true, &mut windows);
        add("minecraft:brewing_stand", 5, 40, 41, -1, true, &mut windows);
        add("minecraft:crafting", 10, 45, 46, 0, true, &mut windows);
        add("minecraft:enchantment", 2, 37, 38, -1, true, &mut windows);
        add("minecraft:furnace", 3, 38, 39, 2, true, &mut windows);
        add("minecraft:grindstone", 3, 38, 39, 2, true, &mut windows);
        add("minecraft:hopper", 5, 40, 41, -1, true, &mut windows);
        add("minecraft:lectern", 1, 36, 37, -1, true, &mut windows);
        add("minecraft:loom", 4, 39, 40, 3, true, &mut windows);
        add("minecraft:merchant", 3, 38, 39, 2, true, &mut windows);
        add("minecraft:shulker_box", 27, 62, 63, -1, true, &mut windows);
        if registry
            .support_feature("netherUpdateInventoryWindows")
            .as_bool()
        {
            add("minecraft:smithing", 3, 38, 39, 2, true, &mut windows);
        }
        add("minecraft:smoker", 3, 38, 39, 2, true, &mut windows);
        add("minecraft:cartography", 3, 38, 39, 2, true, &mut windows);
        add("minecraft:stonecutter", 2, 37, 38, 1, true, &mut windows);
    } else {
        let inv_slots = if registry.support_feature("shieldSlot").as_bool() {
            46
        } else {
            45
        };
        let mk = |key: &str,
                  start: usize,
                  end: usize,
                  slots: usize,
                  craft: i32,
                  windows: &mut HashMap<String, WindowInfo>| {
            windows.insert(
                key.to_string(),
                WindowInfo {
                    key: key.to_string(),
                    inventory_start: start,
                    inventory_end_inclusive: end,
                    slots,
                    craft,
                    require_confirmation: true,
                    type_id: -1,
                },
            );
        };
        mk("minecraft:inventory", 9, 44, inv_slots, 0, &mut windows);
        mk("minecraft:crafting_table", 10, 45, 46, 0, &mut windows);
        mk("minecraft:furnace", 3, 38, 39, 2, &mut windows);
        mk("minecraft:dispenser", 9, 44, 45, -1, &mut windows);
        mk("minecraft:enchanting_table", 2, 37, 38, -1, &mut windows);
        mk("minecraft:brewing_stand", 5, 40, 41, -1, &mut windows);
        mk("minecraft:villager", 3, 38, 39, 2, &mut windows);
        mk("minecraft:beacon", 1, 36, 37, -1, &mut windows);
        mk("minecraft:anvil", 3, 38, 39, 2, &mut windows);
        mk("minecraft:hopper", 5, 40, 41, -1, &mut windows);
        mk("minecraft:dropper", 9, 44, 45, -1, &mut windows);
        mk("minecraft:shulker_box", 27, 62, 63, -1, &mut windows);
    }
    windows
}

/// Create a window from a type identifier (numeric protocol id or string key).
pub fn create_window_from_type(
    registry: &Registry,
    id: i32,
    type_id: i64,
    type_key: Option<&str>,
    title: &str,
    slot_count: Option<usize>,
) -> Option<Window> {
    let types = window_types(registry);
    let win_data = types
        .values()
        .find(|info| info.type_id == type_id)
        .or_else(|| type_key.and_then(|k| types.get(k)));

    match win_data {
        Some(info) => Some(Window::new(
            id,
            info.key.clone(),
            title,
            info.slots,
            info.inventory_start,
            info.inventory_end_inclusive,
            info.craft,
            info.require_confirmation,
        )),
        None => {
            let slot_count = slot_count?;
            Some(Window::new(
                id,
                type_key.unwrap_or("minecraft:container"),
                title,
                slot_count + 36,
                slot_count,
                slot_count + 35,
                -1,
                type_key != Some("minecraft:container"),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{BlockCollisionShapes, ItemDefinition, Registry};

    fn registry() -> Registry {
        Registry::build(
            vec![],
            vec![ItemDefinition {
                id: 1,
                name: "stone".into(),
                display_name: "Stone".into(),
                stack_size: 64,
                enchant_categories: None,
                repair_with: None,
                max_durability: None,
            }],
            vec![],
            vec![],
            vec![],
            vec![],
            BlockCollisionShapes::default(),
            std::collections::HashMap::new(),
            "26.1.2",
        )
    }

    fn stone(reg: &Registry, count: i32) -> Item {
        create_item(reg, 1, count, 0, None, vec![], vec![])
    }

    #[test]
    fn inventory_window_layout() {
        let win =
            create_window_from_type(&registry(), 0, -999, Some("minecraft:inventory"), "", None)
                .unwrap();
        assert_eq!(win.inventory_start, 9);
        assert_eq!(win.inventory_end, 45);
        assert_eq!(win.hotbar_start, 36);
        assert_eq!(win.crafting_result_slot, 0);
    }

    #[test]
    fn left_click_picks_up_and_places() {
        let reg = registry();
        let mut win = Window::new(0, "minecraft:inventory", "", 46, 9, 44, 0, true);
        win.slots[9] = Some(stone(&reg, 32));
        // pick up
        win.mouse_click(
            &reg,
            Click {
                mode: 0,
                mouse_button: 0,
                slot: 9,
            },
        );
        assert!(win.slots[9].is_none());
        assert_eq!(win.selected_item.as_ref().unwrap().count, 32);
        // place into empty slot 10
        win.mouse_click(
            &reg,
            Click {
                mode: 0,
                mouse_button: 0,
                slot: 10,
            },
        );
        assert_eq!(win.slots[10].as_ref().unwrap().count, 32);
        assert!(win.selected_item.is_none());
    }

    #[test]
    fn merging_stacks_respects_stack_size() {
        let reg = registry();
        let mut win = Window::new(0, "minecraft:inventory", "", 46, 9, 44, 0, true);
        win.slots[9] = Some(stone(&reg, 40));
        win.slots[10] = Some(stone(&reg, 40));
        // pick up slot 9, left-click onto slot 10 → fills to 64, 16 left on cursor
        win.mouse_click(
            &reg,
            Click {
                mode: 0,
                mouse_button: 0,
                slot: 9,
            },
        );
        win.mouse_click(
            &reg,
            Click {
                mode: 0,
                mouse_button: 0,
                slot: 10,
            },
        );
        assert_eq!(win.slots[10].as_ref().unwrap().count, 64);
        assert_eq!(win.selected_item.as_ref().unwrap().count, 16);
    }

    #[test]
    fn split_and_count() {
        let reg = registry();
        let mut win = Window::new(0, "minecraft:inventory", "", 46, 9, 44, 0, true);
        win.slots[20] = Some(stone(&reg, 10));
        win.split_slot(&reg, 20);
        assert_eq!(win.selected_item.as_ref().unwrap().count, 5);
        assert_eq!(win.slots[20].as_ref().unwrap().count, 5);
        assert_eq!(win.window_count(1, None), 5);
    }

    /// Decision 7: replay the server's window-0 packets recorded around the 2×2 stick craft that went
    /// wrong (local capture 2026-10-09, rust-gym-008, stale_window_craft; ids 36 oak_planks, 947 stick,
    /// 752 oak_button). The old content handler moved grid items into empty inventory slots on the client
    /// only: after the last packet the client saw an empty grid and a phantom plank in slot 9, while the
    /// server held the plank in grid slot 3 (result: oak_button). The next craft grabbed the phantom, placed
    /// nothing, and crafted a button, 11 times per craft in the race logs. The client must end equal to
    /// the server's final content.
    #[test]
    fn recorded_craft_packets_keep_the_grid() {
        let defs = [(36, "oak_planks"), (752, "oak_button"), (947, "stick")];
        let reg = Registry::build(
            vec![],
            defs.iter()
                .map(|&(id, name)| ItemDefinition { id, name: name.into(), display_name: name.into(), stack_size: 64, enchant_categories: None, repair_with: None, max_durability: None })
                .collect(),
            vec![],
            vec![],
            vec![],
            vec![],
            BlockCollisionShapes::default(),
            std::collections::HashMap::new(),
            "26.1.2",
        );
        let item = |v: &serde_json::Value| -> Option<Item> {
            let a = v.as_array()?;
            Some(create_item(&reg, a[0].as_i64()? as i32, a[1].as_i64()? as i32, 0, None, vec![], vec![]))
        };
        let packets: Vec<serde_json::Value> = serde_json::from_str(include_str!("../tests/fixtures/stale-craft-window.json")).unwrap();
        let mut inv = Window::new(0, "minecraft:inventory", "", 46, 9, 44, 0, true);
        let mut table: Option<Window> = None;
        let mut last_content: Option<Vec<Option<Item>>> = None;
        for p in &packets {
            let wid = p["windowId"].as_i64().unwrap() as i32;
            if p["name"] == "container_set_content" {
                let items: Vec<Option<Item>> = p["items"].as_array().unwrap().iter().map(item).collect();
                if wid == 0 {
                    last_content = Some(items.clone());
                }
                apply_server_content(&mut inv, table.as_mut(), wid, items);
            } else {
                let it = item(&p["item"]);
                if wid == 0 {
                    if let Some(c) = last_content.as_mut() {
                        if let Some(s) = c.get_mut(p["slot"].as_i64().unwrap() as usize) {
                            *s = it.clone();
                        }
                    }
                }
                apply_server_slot(&mut inv, table.as_mut(), wid, p["slot"].as_i64().unwrap() as i32, it);
            }
        }
        let server = last_content.unwrap();
        let view = |w: &[Option<Item>]| w.iter().map(|s| s.as_ref().map(|i| (i.type_id, i.count))).collect::<Vec<_>>();
        assert_eq!(view(&inv.slots), view(&server), "client view differs from the server's last window-0 content");
        assert_eq!(inv.slots[3].as_ref().map(|i| (i.type_id, i.count)), Some((36, 1)), "the server's grid plank in slot 3");
        assert!(inv.slots[9].is_none(), "no phantom plank in slot 9");
    }

    #[test]
    fn container_and_player_window_mirror() {
        let reg = registry();
        let mut inv = Window::new(0, "minecraft:inventory", "", 46, 9, 44, 0, true);
        let mut table = Window::new(3, "minecraft:crafting", "", 46, 10, 45, 0, true);
        // A window-0 update while the table is open (a pickup, a death) reaches the table's copy too…
        apply_server_slot(&mut inv, Some(&mut table), 0, 36, Some(stone(&reg, 7)));
        assert_eq!(table.slots[37].as_ref().map(|i| i.count), Some(7));
        // …and a table update reaches the player window.
        apply_server_slot(&mut inv, Some(&mut table), 3, 10, Some(stone(&reg, 2)));
        assert_eq!(inv.slots[9].as_ref().map(|i| i.count), Some(2));
        // The table's own grid never leaks into the player window.
        apply_server_slot(&mut inv, Some(&mut table), 3, 1, Some(stone(&reg, 1)));
        assert!(inv.slots[1].is_none());
    }

    #[test]
    fn finds_and_counts_items() {
        let reg = registry();
        let mut win = Window::new(0, "minecraft:inventory", "", 46, 9, 44, 0, true);
        win.slots[9] = Some(stone(&reg, 5));
        win.slots[40] = Some(stone(&reg, 3));
        assert_eq!(win.find_inventory_item("stone", None, false), Some(9));
        assert_eq!(win.window_count(1, None), 8);
        assert_eq!(win.empty_slot_count(), 34);
    }
}
