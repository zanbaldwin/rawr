SELECT
    (SELECT COUNT(DISTINCT j.value)
     FROM files f
     INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.fandoms) j
     WHERE f.target = ?1) AS fandom_count,
    (SELECT COUNT(*)
     FROM files f
     INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.tags) j
     WHERE f.target = ?1) AS tag_count,
    (SELECT COUNT(DISTINCT json_extract(j.value, '$.name'))
     FROM files f
     INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.tags) j
     WHERE f.target = ?1) AS unique_tag_count,
    (SELECT COUNT(DISTINCT json_extract(j.value, '$.id'))
     FROM files f
     INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.series) j
     WHERE f.target = ?1) AS series_count,
    (SELECT COUNT(DISTINCT v.work_id)
     FROM files f
     INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.series) j
     WHERE f.target = ?1) AS works_in_series
