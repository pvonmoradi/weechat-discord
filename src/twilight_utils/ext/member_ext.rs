use crate::twilight_utils::color::Color;
use std::sync::Arc;
use twilight_cache_inmemory::{model::CachedMember, InMemoryCache as Cache};
use twilight_model::guild::{Member, Role};

pub trait MemberExt {
    fn color(&self, cache: &Cache) -> Option<Color>;
    fn display_name(&self) -> &str;
    fn highest_role_info(&self, cache: &Cache) -> Option<Arc<Role>>;
}

impl MemberExt for Member {
    fn color(&self, cache: &Cache) -> Option<Color> {
        let mut roles = Vec::new();
        for role in &self.roles {
            if let Some(role) = cache.role(*role) {
                roles.push(role);
            }
        }

        roles.sort_by(|l, r| {
            if r.position == l.position {
                r.id.cmp(&l.id)
            } else {
                r.position.cmp(&l.position)
            }
        });

        let default = 0;
        roles
            .iter()
            .find(|role| role.color != default)
            .map(|role| Color::new(role.color))
    }

    fn display_name(&self) -> &str {
        self.nick.as_ref().unwrap_or(&self.user.name)
    }

    fn highest_role_info(&self, cache: &Cache) -> Option<Arc<Role>> {
        let mut highest: Option<(Arc<Role>, i64)> = None;

        for role_id in &self.roles {
            if let Some(role) = cache.role(*role_id) {
                // Skip this role if this role in iteration has:
                //
                // - a position less than the recorded highest
                // - a position equal to the recorded, but a higher ID
                if let Some((ref highest_role, pos)) = highest {
                    if role.position < pos || (role.position == pos && role.id > highest_role.id) {
                        continue;
                    }
                }

                let pos = role.position;
                highest = Some((role, pos));
            }
        }

        highest.map(|h| h.0)
    }
}

impl MemberExt for CachedMember {
    fn color(&self, cache: &Cache) -> Option<Color> {
        let mut roles = Vec::new();
        for role in &self.roles {
            if let Some(role) = cache.role(*role) {
                roles.push(role);
            }
        }

        roles.sort_by(|l, r| {
            if r.position == l.position {
                r.id.cmp(&l.id)
            } else {
                r.position.cmp(&l.position)
            }
        });

        let default = 0;
        roles
            .iter()
            .find(|role| role.color != default)
            .map(|role| Color::new(role.color))
    }

    fn display_name(&self) -> &str {
        self.nick.as_ref().unwrap_or(&self.user.name)
    }

    fn highest_role_info(&self, cache: &Cache) -> Option<Arc<Role>> {
        let mut highest: Option<(Arc<Role>, i64)> = None;

        for role_id in &self.roles {
            if let Some(role) = cache.role(*role_id) {
                // Skip this role if this role in iteration has:
                //
                // - a position less than the recorded highest
                // - a position equal to the recorded, but a higher ID
                if let Some((ref highest_role, pos)) = highest {
                    if role.position < pos || (role.position == pos && role.id > highest_role.id) {
                        continue;
                    }
                }

                let pos = role.position;
                highest = Some((role, pos));
            }
        }

        highest.map(|h| h.0)
    }
}
