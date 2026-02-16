SELECT DISTINCT v.work_id
FROM versions v
JOIN files f ON f.content_hash = v.content_hash
WHERE f.target = ?
ORDER BY v.work_id
