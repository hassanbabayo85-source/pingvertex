use std::error::Error;

use crate::{commands::UpdateStatusPage, ports::StatusPageRepository};
use domain::{HexColor, entities::StatusPage};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct UpdateStatusPageUseCase<R> {
    repository: R,
}

impl<R> UpdateStatusPageUseCase<R>
where
    R: StatusPageRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, command: UpdateStatusPage) -> UseCaseResult<Option<StatusPage>> {
        let mut status_page = match self
            .repository
            .find_by_id_for_user(command.id, command.user_id)
            .await?
        {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        status_page.rename(command.name)?;
        status_page.set_monitors(command.monitor_ids)?;
        status_page.set_logo_url(command.logo_url)?;

        let primary_color = command.primary_color.map(HexColor::new).transpose()?;
        status_page.set_primary_color(primary_color);

        self.repository.update(&status_page).await?;

        Ok(Some(status_page))
    }
}
