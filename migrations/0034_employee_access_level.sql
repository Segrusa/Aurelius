-- C3: what an employee's runs may do with files and commands. 'inherit' keeps
-- every work tool (what a run had until now); the mode still decides what is asked.
ALTER TABLE employees ADD COLUMN access_level TEXT NOT NULL DEFAULT 'inherit';
