-- SEC-010 fix: toshe race condition (TOCTOU) da za ta iya haifar da
-- incidents na kwafi (duplicates) idan an gudanar da worker replicas
-- fiye da ɗaya a lokaci guda (horizontal scaling).
--
-- Kafin wannan migration, `IncidentManager::open_if_needed` yana yin
-- "duba-sannan-ƙirƙira" (check-then-act) a matakin application kawai:
-- ya duba ko akwai open incident, idan babu sai ya ƙirƙiri sabo. Idan
-- worker biyu suka yi wannan a lokaci guda, duka biyun na iya ganin
-- "babu open incident" su kuma ƙirƙiri incident biyu daban-daban ga
-- monitor guda.
--
-- Wannan partial unique index yana tabbatar da cewa DATABASE DIN KANTA
-- ba za ta taɓa yarda da fiye da incident ɗaya da ba a warware ba
-- (resolved_at IS NULL) ga kowane monitor_id ba — koda application-level
-- check ɗin ya "kasa" saboda race condition.
CREATE UNIQUE INDEX IF NOT EXISTS idx_incidents_unique_open_per_monitor
    ON incidents (monitor_id)
    WHERE resolved_at IS NULL;
