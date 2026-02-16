SELECT v.work_id, COUNT(*) as count
FROM versions v
GROUP BY v.work_id
HAVING count > 1
ORDER BY count DESC
