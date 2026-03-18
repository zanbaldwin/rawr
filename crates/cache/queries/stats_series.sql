SELECT
    COUNT(DISTINCT json_extract(j.value, '$.id')) AS series_count,
    COUNT(DISTINCT v.work_id) AS works_in_series
FROM files f
INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.series) j
WHERE f.target = ?1
