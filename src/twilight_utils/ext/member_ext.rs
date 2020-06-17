use crate::twilight_utils::color::Color;
use async_trait::async_trait;
use twilight::cache::{twilight_cache_inmemory::model::CachedMember, InMemoryCache as Cache};

#[async_trait]
pub trait MemberExt {
    async fn color(&self, cache: &Cache) -> Option<Color>;
}

#[async_trait]
impl MemberExt for CachedMember {
    async fn color(&self, cache: &Cache) -> Option<Color> {
        let mut roles = Vec::new();
        for role in &self.roles {
            if let Some(role) = cache.role(*role).await.expect("InMemoryCache cannot fail") {
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
}
