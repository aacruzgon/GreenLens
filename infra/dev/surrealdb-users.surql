-- The `api` database user for a local api against dev.compose.yml. Throwaway credentials:
-- the password is `api`, and nothing here is used outside a laptop. Production users are created by
-- infra/bootstrap/surrealdb-users.sh (docs/infra/README.md section 7).
-- `surreal import` refuses a file that doesn't start with OPTION IMPORT.
OPTION IMPORT;
DEFINE NAMESPACE IF NOT EXISTS pdm;
USE NS pdm;
DEFINE DATABASE IF NOT EXISTS main;
USE DB main;
DEFINE USER IF NOT EXISTS api ON DATABASE PASSWORD 'api' ROLES EDITOR;
