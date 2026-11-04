#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: [u8; 16],
    pub title: String,
    pub color: i32,
    pub collapsed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub index: usize,
    pub title: String,
    pub active: bool,
    pub group: Option<Group>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Group(Group),
    Tab {
        index: usize,
        title: String,
        active: bool,
        child: bool,
    },
    NewTab,
    /// The title of the tabs a split shows, which sit together below it as one group. `count` is
    /// how many follow.
    SplitTitle { count: usize },
}
/// The height of a row of the tab list in DIPs. The split group's title is lower than a tab.
pub fn height(row: &Row) -> f32 {
    match row {
        Row::SplitTitle { .. } => SPLIT_TITLE_HEIGHT,
        _ => 34.0,
    }
}
pub const SPLIT_TITLE_HEIGHT: f32 = 24.0;
/// Gathers the tabs that `split` shows, in its reading order, into one "SPLIT VIEW" group at the
/// place of the first of them. Fewer than two tabs make no group.
pub fn group_split(rows: Vec<Row>, split: &[usize]) -> Vec<Row> {
    let in_split = |row: &Row| matches!(row, Row::Tab { index, .. } if split.contains(index));
    let members: Vec<Row> = split
        .iter()
        .filter_map(|wanted| rows.iter().find(|row| matches!(row, Row::Tab { index, .. } if index == wanted)).cloned())
        .collect();
    if members.len() < 2 {
        return rows;
    }
    let Some(place) = rows.iter().position(in_split) else { return rows };
    let mut out: Vec<Row> = Vec::with_capacity(rows.len() + 1);
    for (at, row) in rows.into_iter().enumerate() {
        if at == place {
            out.push(Row::SplitTitle { count: members.len() });
            out.extend(members.iter().cloned().map(|member| match member {
                Row::Tab { index, title, active, .. } => Row::Tab { index, title, active, child: true },
                other => other,
            }));
        }
        if !in_split(&row) {
            out.push(row);
        }
    }
    out
}
pub fn build_rows(tabs: &[Tab]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut seen = Vec::new();
    for tab in tabs {
        if let Some(group) = &tab.group {
            if seen.contains(&group.id) {
                continue;
            }
            seen.push(group.id);
            rows.push(Row::Group(group.clone()));
            if !group.collapsed {
                rows.extend(
                    tabs.iter()
                        .filter(|t| t.group.as_ref().is_some_and(|g| g.id == group.id))
                        .map(|t| Row::Tab {
                            index: t.index,
                            title: t.title.clone(),
                            active: t.active,
                            child: true,
                        }),
                );
            }
        }
    }
    rows.push(Row::NewTab);
    rows.extend(tabs.iter().filter(|t| t.group.is_none()).map(|t| Row::Tab {
        index: t.index,
        title: t.title.clone(),
        active: t.active,
        child: false,
    }));
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tab(index: usize, group: Option<Group>) -> Tab {
        Tab {
            index,
            title: format!("Tab {index}"),
            active: false,
            group,
        }
    }
    fn group(collapsed: bool) -> Group {
        Group {
            id: [1; 16],
            title: "Work".into(),
            color: 3,
            collapsed,
        }
    }
    #[test]
    fn fr3_no_groups_show_only_new_and_ungrouped_tabs() {
        let rows = build_rows(&[tab(0, None)]);
        assert!(matches!(
            &rows[..],
            [
                Row::NewTab,
                Row::Tab {
                    index: 0,
                    child: false,
                    ..
                }
            ]
        ));
    }
    #[test]
    fn fr3_groups_show_their_children_once_and_keep_tab_indices() {
        let rows = build_rows(&[
            tab(0, None),
            tab(1, Some(group(false))),
            tab(2, Some(group(false))),
        ]);
        assert!(matches!(
            &rows[..],
            [
                Row::Group(_),
                Row::Tab {
                    index: 1,
                    child: true,
                    ..
                },
                Row::Tab {
                    index: 2,
                    child: true,
                    ..
                },
                Row::NewTab,
                Row::Tab {
                    index: 0,
                    child: false,
                    ..
                }
            ]
        ));
    }
    #[test]
    fn fr3_collapsed_group_hides_its_children() {
        let rows = build_rows(&[tab(0, Some(group(true)))]);
        assert!(matches!(&rows[..], [Row::Group(_), Row::NewTab]));
    }
    #[test]
    fn fr9_split_tabs_show_as_one_split_view_group_in_reading_order() {
        let rows = build_rows(&[tab(0, None), tab(1, None), tab(2, None), tab(3, None)]);
        let rows = group_split(rows, &[3, 1]);
        let shape: Vec<(Option<usize>, bool)> = rows
            .iter()
            .map(|row| match row {
                Row::Tab { index, child, .. } => (Some(*index), *child),
                Row::SplitTitle { count } => (None, *count == 2),
                _ => (None, false),
            })
            .collect();
        assert_eq!(shape, [(None, false), (Some(0), false), (None, true), (Some(3), true), (Some(1), true), (Some(2), false)]);
        assert_eq!(height(&rows[2]), SPLIT_TITLE_HEIGHT);
    }
    #[test]
    fn fr9_a_window_without_a_split_shows_no_split_group() {
        let rows = build_rows(&[tab(0, None), tab(1, None)]);
        assert_eq!(group_split(rows.clone(), &[]), rows);
        assert_eq!(group_split(rows.clone(), &[1]), rows);
    }
}
