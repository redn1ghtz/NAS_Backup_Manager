"""SQL contract tests: in-memory databases only, Python standard library."""
import pathlib
import sqlite3
import unittest

SCHEMA = (pathlib.Path(__file__).resolve().parents[1]
          / 'crates/storage/migrations/0001_initial.sql').read_text(encoding='utf-8')


class SchemaTests(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(':memory:')
        self.addCleanup(self.db.close)
        self.db.execute('PRAGMA foreign_keys=ON')
        self.db.executescript(SCHEMA)
        self.job('job-1')

    def job(self, identifier):
        self.db.execute('''INSERT INTO jobs
            (id,name,source_path,destination_path,destination_kind,created_at,updated_at)
            VALUES (?, 'Documents', '/source', '/destination', 'local', 1, 1)''', (identifier,))

    def add_run(self, identifier='run-1', job='job-1'):
        self.db.execute('''INSERT INTO runs
            (id,job_id,job_revision,trigger_kind,state,started_at)
            VALUES (?, ?, 1, 'preview', 'scanning', 1)''', (identifier, job))

    def archive(self, identifier, stored):
        self.db.execute('''INSERT INTO archive_versions
            (id,job_id,run_id,original_relative_path,stored_relative_path,
             reason,archived_at,expires_at,byte_count,state)
            VALUES (?, 'job-1', 'run-1', 'Документы/report.txt', ?, 'deleted', 1, 2, 12, 'verified')''',
                        (identifier, stored))

    def test_disabled_until_verified_preview(self):
        self.assertEqual(self.db.execute('SELECT enabled,schedule_enabled FROM jobs').fetchone(), (0, 0))
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('UPDATE jobs SET enabled=1')
        self.db.execute("""UPDATE jobs SET approved_revision=revision,
            expected_identity='volume-id',ownership_nonce='nonce',enabled=1""")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('UPDATE jobs SET revision=2')

    def test_invalid_retention_and_schedule(self):
        for assignment in ['retention_days=0', 'daily_minute=1440', 'schedule_enabled=1',
                           "destination_kind='smb-password'", "missed_run_policy='burst'"]:
            with self.assertRaises(sqlite3.IntegrityError):
                self.db.execute('UPDATE jobs SET ' + assignment)

    def test_manual_and_scheduled_runs_cannot_overlap(self):
        self.add_run()
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_run('run-2')
        self.job('job-2')
        self.add_run('run-3', 'job-2')
        self.db.execute("UPDATE runs SET state='interrupted' WHERE id='run-1'")
        self.add_run('run-2')

    def test_failed_scan_cannot_report_success(self):
        self.add_run()
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE runs SET state='succeeded',finished_at=2")
        self.db.execute("UPDATE runs SET scan_complete=1,state='succeeded',finished_at=2")

    def test_archive_survives_job_removal_and_preserves_versions(self):
        self.add_run()
        self.archive('version-1', 'version-1/report.txt')
        self.archive('version-2', 'version-2/report.txt')
        with self.assertRaises(sqlite3.IntegrityError):
            self.archive('version-3', 'version-1/report.txt')
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("DELETE FROM jobs WHERE id='job-1'")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("DELETE FROM runs WHERE id='run-1'")
        self.db.execute('UPDATE jobs SET removed_at=3')
        self.assertEqual(self.db.execute('SELECT count(*) FROM archive_versions').fetchone()[0], 2)

    def test_foreign_keys_and_integrity(self):
        self.assertEqual(self.db.execute('PRAGMA integrity_check').fetchone()[0], 'ok')
        self.assertEqual(self.db.execute('PRAGMA foreign_key_check').fetchall(), [])
        self.assertEqual(self.db.execute('SELECT version FROM schema_migrations').fetchall(), [(1,)])

    def test_archive_cannot_reference_another_jobs_run(self):
        self.job('job-2')
        self.add_run(job='job-2')
        with self.assertRaises(sqlite3.IntegrityError):
            self.archive('version-1', 'version-1/report.txt')


if __name__ == '__main__':
    unittest.main()
