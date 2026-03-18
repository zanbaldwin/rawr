WITH tag_data AS (
    SELECT json_extract(j.value, '$.name') AS name
    FROM files f
    INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.tags) j
    WHERE f.target = ?1
)
SELECT
    (SELECT COUNT(*) FROM tag_data) AS tag_count,
    (SELECT COUNT(DISTINCT name) FROM tag_data) AS unique_tag_count
