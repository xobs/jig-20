Localization
------------

Unit files may include localization.  If a localized string is available, then it will be used.  Otherwise, it will fall back to the non-localized string.

For example:

    Name=Log Registers
    Name[zh]=登录寄存器

TestEntry
---------

A TestEntry is one available test.

TestEntry objects have hard and soft dependencies.  For example, it could be that you want to run a color LCD test after running a sound test.  But if the sound test fails, you still want to run the color LCD test.  However, both depend on the firmware having been programmed.  Firmware programming is a hard depenency, and the sound test is a soft depenency.

The TestEntry object is configured with a ".test" file.  They are very similar to systemd unit files.

Test specifications are defined under a "[Test]" section.
* Name: Defines the short name for this test.
* Description: Defines a detailed description of this test.  May be up to one paragraph.
* Requires: The name of a test that must successfully complete
* Suggests: The name of a test that should be run first, but is not catastrophic if it fails
* Timeout: The maximum number of seconds that this test may be run for.
* Type: One of "simple" or "daemon".  For "simple" tests, the return code will indicate pass or fail, and each line printed will be considered progress.  For "daemon", the process will be forked and left to run in the background.  See "daemons" below.
* ExecStart: The command to run as part of this test.
* ExecStopFail: When stopping tests, if the test failed, then this stop command will be run.
* ExecStopSuccess: When stopping tests, if the test succeeded, then this stop command will be run.
* ExecStop: When tests are completed, this command is run to clean things up.  If either ExecStopSuccess or ExecStopFail are present, then this command will be skipped.

Daemons
-------

Some tests can run as daemons.  This is particularly useful for services such as OpenOCD, which must be set up in order to program some devices.

TestPlan
--------

TestPlans are defined in a .plan file.  They are similar to systemd unit files.

The following fields are allowed in the [Plan] section:
* Name: The name of the product or device being tested.
* Description: A longer description of the product or device being tested, up to one paragraph.
* Tests: A space- or comma-separated list of tests to be run.  Note that you only need to specify the final test to run, as the dependency graph will fill in the rest.  If you specify multiple tests, then they will be run in the order you specify, possibly with dependency tests added in between.
* Success: A command to run if a test plan completes successfully.
* Failure: A command to be run if a test plan fails.

TestSet
-------

A TestSet is an unordered set of tests and plans that the system knows about.

In a future version, the tester may actually monitor the directory for files, but as of now, the tester must be restarted for it to pick up new tests.

TestSequence
------------

An ordered series of TestEntry objects that may be executed.