use crate::codex::NormalizedEvent;
use crate::error::{Error, ErrorKind, Result};
use crate::protocol::{Event, EventSource, PROTOCOL_VERSION};
use crate::reducer::{Projection, apply};
use crate::store::{Database, EventInput};
use serde_json::Value;

pub(crate) struct TaskJournal<'a> {
    pub(crate) projection: Projection,
    pub(crate) events: Vec<Event>,
    pub(crate) database: Option<&'a Database>,
}

impl TaskJournal<'_> {
    pub(crate) async fn append(
        &mut self,
        kind: &str,
        data: Value,
        source: Option<EventSource>,
        source_key: Option<String>,
        observed_at: String,
    ) -> Result<Event> {
        if let Some(database) = self.database {
            let outcome = database
                .append(EventInput {
                    task_id: self.projection.task_id.clone(),
                    attempt: self.projection.attempt,
                    kind: kind.to_owned(),
                    data,
                    source,
                    source_key,
                    observed_at,
                })
                .await?;
            self.projection = outcome.projection;
            if outcome.inserted {
                self.events.push(outcome.event.clone());
            }
            return Ok(outcome.event);
        }
        let event = self.preview(kind, data, source, observed_at)?;
        self.projection = apply(&self.projection, &event)?;
        self.events.push(event.clone());
        Ok(event)
    }

    pub(crate) async fn append_normalized(&mut self, event: NormalizedEvent) -> Result<Event> {
        self.append(
            &event.kind,
            event.data,
            Some(event.source),
            Some(event.source_key),
            event.observed_at,
        )
        .await
    }

    pub(crate) fn preview_normalized(&self, event: &NormalizedEvent) -> Result<Projection> {
        let preview = self.preview(
            &event.kind,
            event.data.clone(),
            Some(event.source.clone()),
            event.observed_at.clone(),
        )?;
        apply(&self.projection, &preview)
    }

    fn preview(
        &self,
        kind: &str,
        data: Value,
        source: Option<EventSource>,
        observed_at: String,
    ) -> Result<Event> {
        let seq = self
            .projection
            .event_seq
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "event sequence exhausted"))?;
        Ok(Event {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            task_id: self.projection.task_id.clone(),
            attempt: self.projection.attempt,
            seq,
            kind: kind.to_owned(),
            observed_at,
            data,
            source,
        })
    }
}
