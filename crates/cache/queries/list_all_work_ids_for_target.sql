SELECT DISTINCT v.work_id
FROM versions v
JOIN files f ON f.content_hash = v.content_has
WHERE f.target = ?
ORDER BY v.work_id
