-- Cheap change-detection aggregates for a storage target.
--
-- Five aggregates that together move whenever the target's contents change:
-- file count, newest discovery time and total compressed bytes for the
-- target, plus version count and newest extraction time overall. Callers
-- compare whole rows; the values are never shown to users.
SELECT
    (SELECT COUNT(*) FROM files WHERE target = ?1)                        AS file_count,
    (SELECT COALESCE(MAX(discovered_at), 0) FROM files WHERE target = ?1) AS latest_discovered_at,
    (SELECT COALESCE(SUM(file_size), 0) FROM files WHERE target = ?1)     AS total_file_size,
    (SELECT COUNT(*) FROM versions)                                       AS version_count,
    (SELECT COALESCE(MAX(extracted_at), 0) FROM versions)                 AS latest_extracted_at
