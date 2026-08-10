use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ID-newtype (арх. § 5): перепутать идентификаторы разных сущностей
/// не дает компилятор. В БД - uuid v7.
macro_rules! define_id {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new(id: Uuid) -> Self {
                Self(id)
            }

            pub fn into_uuid(self) -> Uuid {
                self.0
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    )+};
}

define_id! {
    /// Тендер (core.tenders)
    TenderId,
    /// Лот тендера (core.lots)
    LotId,
    /// Объект имущества (core.objects)
    ObjectId,
    /// Пользователь (core.users)
    UserId,
    /// Заявка участника (core.applications)
    ApplicationId,
    /// Торги по лоту (core.auctions)
    AuctionId,
    /// Ставка в торгах (core.bids)
    BidId,
}
