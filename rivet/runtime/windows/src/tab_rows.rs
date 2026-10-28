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
}
