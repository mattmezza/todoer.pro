use super::*;
use rusqlite::OptionalExtension;
#[derive(Clone)]
pub(super) struct SidebarList {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub color: String,
}
#[derive(Clone)]
pub(super) struct SidebarSpace {
    pub writable: bool,
    pub id: String,
    pub title: String,
    pub slug: String,
}
pub(super) struct Sidebar {
    pub space_overview: bool,
    pub settings_section: String,
    pub favorites: Vec<SidebarList>,
    pub favorite_ids: Vec<String>,
    pub list_is_favorite: bool,
    pub account_id: String,
    pub lists: Vec<SidebarList>,
    pub spaces: Vec<SidebarSpace>,
    pub menu_spaces: Vec<SidebarSpace>,
    pub user_name: String,
    pub user_initial: String,
    pub avatar_url: String,
    pub current_workspace_label: String,
    pub current_workspace_id: String,
    pub current_list_id: String,
    pub csrf: String,
}
pub(super) fn build_sidebar(
    db: &Store,
    actor: &str,
    current_list: &str,
    csrf: String,
) -> Result<Sidebar, Error> {
    let (user, _) = crate::core::actor_identity(&db.conn, actor)?;
    let user_name: String =
        db.conn
            .query_row("SELECT name FROM users WHERE id=?1", [&user], |r| r.get(0))?;
    let profile: Option<(String, String, Option<String>)> = db.conn.query_row(
        "SELECT issuer,email,picture_url FROM auth_identities WHERE user_id=?1 ORDER BY CASE WHEN issuer IN ('https://accounts.google.com','accounts.google.com') THEN 0 ELSE 1 END,issuer,subject LIMIT 1", [&user],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional()?;
    let avatar_url = match profile {
        Some((issuer, _, picture))
            if issuer == "https://accounts.google.com" || issuer == "accounts.google.com" =>
        {
            picture
                .filter(|url| url.starts_with("https://"))
                .unwrap_or_default()
        }
        Some((_, email, _)) if !email.trim().is_empty() => format!(
            "https://www.gravatar.com/avatar/{:x}?s=80&d=404",
            Sha256::digest(email.trim().to_lowercase().as_bytes())
        ),
        _ => String::new(),
    };
    let slugs = db.entity_slugs(actor)?;
    let mut editable = std::collections::HashSet::<String>::new();
    let mut ranks = db
        .conn
        .prepare("SELECT workspace_id FROM actor_workspace_access WHERE actor_id=?1 AND rank>=2")?;
    for row in ranks.query_map([actor], |r| r.get::<_, String>(0))? {
        editable.insert(row?);
    }
    let spaces = db
        .workspaces(actor)?
        .into_iter()
        .map(|w| SidebarSpace {
            writable: editable.contains(&w.id),
            id: w.id.clone(),
            slug: slugs
                .workspaces
                .get(&w.id)
                .cloned()
                .unwrap_or_else(|| w.title.to_lowercase()),
            title: w.title,
        })
        .collect::<Vec<_>>();
    let mut colors = std::collections::HashMap::<String, String>::new();
    let mut q=db.conn.prepare("SELECT l.id,coalesce(l.color_hex,l.color,'blue') FROM lists l JOIN actor_list_access a ON a.list_id=l.id WHERE a.actor_id=?1")?;
    for row in q.query_map([actor], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (id, color) = row?;
        colors.insert(id, color);
    }
    let workspace: Option<String> = db
        .conn
        .query_row(
            "SELECT workspace_id FROM lists WHERE id=?1",
            [current_list],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let current_workspace_label = spaces
        .iter()
        .find(|s| Some(&s.id) == workspace.as_ref())
        .map(|s| format!("@{}", s.slug))
        .or_else(|| {
            spaces
                .iter()
                .find(|space| space.slug == "inbox")
                .or_else(|| spaces.first())
                .map(|space| format!("@{}", space.slug))
        })
        .unwrap_or_else(|| "@inbox".into());
    let lists = db
        .lists(actor)?
        .into_iter()
        .filter(|l| {
            workspace.as_ref().is_some_and(|w| {
                db.conn
                    .query_row(
                        "SELECT workspace_id=?2 FROM lists WHERE id=?1",
                        rusqlite::params![l.id, w],
                        |r| r.get::<_, bool>(0),
                    )
                    .unwrap_or(false)
            })
        })
        .map(|l| SidebarList {
            id: l.id.clone(),
            slug: slugs
                .lists
                .get(&l.id)
                .cloned()
                .unwrap_or_else(|| l.title.to_lowercase()),
            color: colors.remove(&l.id).unwrap_or_else(|| "blue".into()),
            title: l.title,
        })
        .collect();
    let personal_favorites = db.favorites(actor)?;
    let favorite_ids: Vec<String> = personal_favorites
        .iter()
        .filter(|f| f.kind == "list")
        .map(|f| f.id.clone())
        .collect();
    let favorites = personal_favorites
        .into_iter()
        .filter(|f| f.kind == "list")
        .map(|f| SidebarList {
            slug: slugs
                .lists
                .get(&f.id)
                .cloned()
                .unwrap_or_else(|| f.title.to_lowercase()),
            color: db
                .list_color(actor, &f.id)
                .ok()
                .flatten()
                .unwrap_or_else(|| "blue".into()),
            id: f.id,
            title: f.title,
        })
        .collect();
    let archived_spaces = db.conn.prepare("SELECT workspace_id FROM workspace_lifecycle WHERE archived=1 OR deleted_at IS NOT NULL")?
        .query_map([], |r| r.get::<_, String>(0))?.collect::<Result<std::collections::HashSet<_>, _>>()?;
    let menu_spaces = spaces
        .iter()
        .filter(|s| Some(&s.id) == workspace.as_ref())
        .chain(spaces.iter().filter(|s| Some(&s.id) != workspace.as_ref()))
        .filter(|s| !archived_spaces.contains(&s.id))
        .take(10)
        .cloned()
        .collect();
    Ok(Sidebar {
        menu_spaces,
        space_overview: false,
        settings_section: String::new(),
        list_is_favorite: favorite_ids.iter().any(|id| id == current_list),
        favorite_ids,
        favorites,
        account_id: user,
        lists,
        spaces: spaces.clone(),
        user_initial: user_name
            .chars()
            .next()
            .unwrap_or('T')
            .to_uppercase()
            .collect(),
        user_name,
        avatar_url,
        current_workspace_id: spaces
            .iter()
            .find(|s| Some(&s.id) == workspace.as_ref())
            .map(|s| s.id.clone())
            .unwrap_or_default(),
        current_workspace_label,
        current_list_id: current_list.into(),
        csrf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gravatar_uses_normalized_verified_identity_and_real_workspace_slug() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn.execute("INSERT INTO auth_identities(issuer,subject,user_id,email) VALUES('https://example.test','synthetic-subject','dev',' USER@EXAMPLE.INVALID ')",[]).unwrap();
        let workspace = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Personal Work".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let list = db
            .create_list(
                "dev",
                crate::workspace::CreateList {
                    workspace_id: workspace.id,
                    title: "Reading".into(),
                    description: String::new(),
                    restricted: false,
                },
            )
            .unwrap();
        let sidebar = build_sidebar(&db, "dev", &list.id, "synthetic-csrf".into()).unwrap();
        assert_eq!(sidebar.current_workspace_label, "@personal-work");
        assert_eq!(
            sidebar.avatar_url,
            "https://www.gravatar.com/avatar/34657fd697457c69244beb91db2c5bc3ae65ac924fd3f7843b9816057ab9f988?s=80&d=404"
        );
        assert!(!sidebar.avatar_url.contains("user@"));
    }
    #[test]
    fn google_profile_uses_provider_picture_and_never_gravatar() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        db.conn.execute("INSERT INTO auth_identities(issuer,subject,user_id,email,picture_url) VALUES('https://accounts.google.com','subject','dev','user@example.test','https://lh3.googleusercontent.com/synthetic-photo')",[]).unwrap();
        assert_eq!(
            build_sidebar(&db, "dev", "inbox", String::new())
                .unwrap()
                .avatar_url,
            "https://lh3.googleusercontent.com/synthetic-photo"
        );
        db.conn
            .execute("UPDATE auth_identities SET picture_url=NULL", [])
            .unwrap();
        assert!(
            build_sidebar(&db, "dev", "inbox", String::new())
                .unwrap()
                .avatar_url
                .is_empty()
        );
    }
    #[test]
    fn direct_list_guest_never_receives_private_workspace_label_or_owner_avatar() {
        let mut db = Store::memory().unwrap();
        db.seed_dev().unwrap();
        let workspace = db
            .create_workspace(
                "dev",
                crate::workspace::WorkspaceInput {
                    title: "Private workspace name".into(),
                    description: String::new(),
                },
            )
            .unwrap();
        let list = db
            .create_list(
                "dev",
                crate::workspace::CreateList {
                    workspace_id: workspace.id,
                    title: "Shared reading".into(),
                    description: String::new(),
                    restricted: false,
                },
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO memberships(list_id,user_id) VALUES(?1,'other')",
                [&list.id],
            )
            .unwrap();
        let sidebar = build_sidebar(&db, "other", &list.id, "synthetic-csrf".into()).unwrap();
        assert!(sidebar.spaces.is_empty());
        assert!(!sidebar.current_workspace_label.contains("private"));
        assert!(sidebar.avatar_url.is_empty());
    }
}
