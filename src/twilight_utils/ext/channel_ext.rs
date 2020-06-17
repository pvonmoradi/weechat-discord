use twilight::model::{
    channel::{ChannelType, Group, GuildChannel, PrivateChannel},
    id::ChannelId,
};

pub trait ChannelExt {
    fn name(&self) -> String;
    fn id(&self) -> ChannelId;
    fn kind(&self) -> ChannelType;
}

impl ChannelExt for GuildChannel {
    fn name(&self) -> String {
        match self {
            GuildChannel::Category(c) => c.name.clone(),
            GuildChannel::Text(c) => c.name.clone(),
            GuildChannel::Voice(c) => c.name.clone(),
        }
    }

    fn id(&self) -> ChannelId {
        match self {
            GuildChannel::Category(c) => c.id,
            GuildChannel::Text(c) => c.id,
            GuildChannel::Voice(c) => c.id,
        }
    }

    fn kind(&self) -> ChannelType {
        match self {
            GuildChannel::Category(c) => c.kind,
            GuildChannel::Text(c) => c.kind,
            GuildChannel::Voice(c) => c.kind,
        }
    }
}

impl ChannelExt for PrivateChannel {
    fn name(&self) -> String {
        let recipient = self
            .recipients
            .first()
            .expect("private channel must have exactly one recipient");
        format!("DM with {}#{:04}", recipient.name, recipient.discriminator)
    }

    fn id(&self) -> ChannelId {
        self.id
    }

    fn kind(&self) -> ChannelType {
        self.kind
    }
}

impl ChannelExt for Group {
    fn name(&self) -> String {
        format!(
            "DM with {}",
            self.recipients
                .iter()
                .map(|recip| format!("{}#{:04}", recip.name, recip.discriminator))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn id(&self) -> ChannelId {
        self.id
    }

    fn kind(&self) -> ChannelType {
        self.kind
    }
}
