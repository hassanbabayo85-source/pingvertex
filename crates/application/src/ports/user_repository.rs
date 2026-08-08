use std::error::Error;

use async_trait::async_trait;
use domain::{Email, User, UserId, Username};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(&self, user: &User) -> RepoResult<()>;
    async fn update(&self, user: &User) -> RepoResult<()>;
    async fn find_by_id(&self, id: &UserId) -> RepoResult<Option<User>>;
    async fn find_by_email(&self, email: &Email) -> RepoResult<Option<User>>;
    async fn find_by_username(&self, username: &Username) -> RepoResult<Option<User>>;
    async fn delete(&self, id: &UserId) -> RepoResult<()>;
    async fn email_exists(&self, email: &Email) -> RepoResult<bool>;
    async fn username_exists(&self, username: &Username) -> RepoResult<bool>;
}

use std::sync::Arc;

#[async_trait]
impl<T> UserRepository for Arc<T>
where
    T: UserRepository + ?Sized,
{
    async fn create(&self, user: &domain::User) -> RepoResult<()> {
        (**self).create(user).await
    }

    async fn update(&self, user: &domain::User) -> RepoResult<()> {
        (**self).update(user).await
    }

    async fn find_by_id(&self, id: &domain::UserId) -> RepoResult<Option<domain::User>> {
        (**self).find_by_id(id).await
    }

    async fn find_by_email(&self, email: &domain::Email) -> RepoResult<Option<domain::User>> {
        (**self).find_by_email(email).await
    }

    async fn find_by_username(
        &self,
        username: &domain::Username,
    ) -> RepoResult<Option<domain::User>> {
        (**self).find_by_username(username).await
    }

    async fn delete(&self, id: &domain::UserId) -> RepoResult<()> {
        (**self).delete(id).await
    }

    async fn email_exists(&self, email: &domain::Email) -> RepoResult<bool> {
        (**self).email_exists(email).await
    }

    async fn username_exists(&self, username: &domain::Username) -> RepoResult<bool> {
        (**self).username_exists(username).await
    }
}
