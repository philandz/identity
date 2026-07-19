-- CI MySQL init script.
--
-- libs@sandbox's philand_table constants reference philandz.<table>, so
-- identity biz queries hit the philandz schema. The MySQL docker image
-- only auto-creates the philand DB (the one named in MYSQL_DATABASE),
-- so we also create philandz here and grant the test user privileges.
-- Without this, integration tests panic with INSERT command denied
-- on philandz.users.
CREATE DATABASE IF NOT EXISTS philandz;
GRANT ALL PRIVILEGES ON philandz.* TO 'philand'@'%';
GRANT ALL PRIVILEGES ON philandz.* TO 'philand'@'localhost';
GRANT ALL PRIVILEGES ON philandz.* TO 'philand'@'127.0.0.1';
FLUSH PRIVILEGES;
