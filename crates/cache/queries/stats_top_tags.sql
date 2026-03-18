SELECT kind, name, count
FROM (
    SELECT
        json_extract(j.value, '$.kind') AS kind,
        json_extract(j.value, '$.name') AS name,
        COUNT(DISTINCT v.work_id) AS count,
        ROW_NUMBER() OVER (
            PARTITION BY json_extract(j.value, '$.kind')
            ORDER BY COUNT(DISTINCT v.work_id) DESC
        ) AS rn
    FROM files f
    INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.tags) j
    WHERE f.target = ?1
    GROUP BY kind, name
)
WHERE rn <= ?2
ORDER BY kind, count DESC
