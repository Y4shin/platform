-- M14 Stage A: per-user locale preference.
--
-- NULL means "fall back to Accept-Language → deployment default". The host's
-- `LocaleResolver` consumes this column when computing the request locale and
-- when resolving the recipient locale for asynchronous work (jobs / .ics feeds).

ALTER TABLE platform.user ADD COLUMN locale TEXT NULL;
