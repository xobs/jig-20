
Unit File Formats
=================

Unit files all live in the configuration directory.  They have distinct suffixes.

Unit files refer to other unit files by filename.  You may omit the suffix if it is unambiguous.

.test
-----

.test files describe Tests, which are atomic, Fundamental units of test.

Test objects have hard and soft dependencies.  For example, it could be that you want to run a color LCD test after running a sound test.  But if the sound test fails, you still want to run the color LCD test.  However, both depend on the firmware having been programmed.  Firmware programming is a hard depenency, and the sound test is a soft depenency.

Fields:

Test specifications are defined under a "[Test]" section.
* Name: Defines the short display name for this test.
* Description: Defines a detailed description of this test.  May be up to one paragraph.
* Requires: A comma- or space-separated list of names of tests that must successfully complete in order to run this test
* Suggests: A comma- or space-separated list of names of tests that should be run first, but is not catastrophic if they fail
* Provides: A comma-separated list of tests that this test can act as.  For example, you may have a test on a Raspberry Pi called 'openocd-rpi' that can Provide "swd".  On a desktop system, you might use 'openocd-olimex' to Provide "swd".
* Timeout: The maximum number of seconds that this test may be run for before it times out, is killed, and marked failure.
* Type: One of "simple" or "daemon".  For "simple" tests, the return code will indicate pass or fail, and each line printed will be considered progress.  For "daemon", the testing procedure will continue as soon as DaemonReadyText is read on stdout.  The daemon must not call fork()/exit(), and must remain in the foreground.
* DaemonReadyText: A string to look for on the test's stdout to indicate the daemon is ready.
* CompatibleJigs: A comma-separated list of jigs that this test is compatible with.  If unspecified, any jig is acceptable.
* ExecStart: The command to run as part of this test.
* ExecStopFail: When stopping tests, if the test failed, then this stop command will be run.
* ExecStopSuccess: When stopping tests, if the test succeeded, then this stop command will be run.
* ExecStop: When tests are completed, this command is run to clean things up.  If either ExecStopSuccess or ExecStopFail are present, then this command will be skipped.

.jig
----

Jigs are physical devices that perform tests.  You will have a jig in the factory, and you should have a jig in your workshop.  Your work machine can also act as a "Jig", though it might not provide all of the same features.

The following fields are allowed in the [Jig] section:
* Name: The name of the jig and the device or product that it tests.
* Description: A longer description of the jig and the product or device being tested, up to one paragraph long.
* TestProgram: Optional path to a program to determine if this is the jig we're running on.
* WorkingDirectory: Directory to run the test program from.
* DefaultWorkingDirectory: A default directory to run tests from.
* TestFile: Optional path to a file to determine if this is the jig we're running on.  If both TestFile and TestProgram are specified, then they must both pass for this to be true.
* DefaultScenario: The name of the scenario to run by default.


.scenario
---------
* Name: A short string describing what this scenario does, for example "test wifi" or "final factory test"
* Description: Longer paragraph describing this scenario
* Tests: A space- or comma-separated list of tests to be run.  Note that you only need to specify the final test to run, as the dependency graph will fill in the rest.  If you specify multiple tests, then they will be run in the order you specify, possibly with dependency tests added in between.
* ExecStart: A command to be run when the scenario is first started.
* ExecStopSuccess: A command to run if a test scenario completes successfully.
* ExecStopFailure: A command to be run if a test scenario fails.
* WorkingDirectory: Directory to run the programs from.
* Timeout: Maximum number of seconds this scenario should take.


.trigger
--------

A trigger is used to start a test.  Triggers are non-repeating and events are consumed.  That is, you can send as many "start" commands as you like, but if the test is already running then they will be ignored.

The following fields are valid in the [Trigger] section:
* Name: A short string describing this trigger
* Description: A longer decsription of this trigger, up to one paragraph long.
* ExecStart: Name of the program to run to get trigger information from.
* WorkingDirectory: Directory to run the ExecStart program from.
* Jigs: A comma-separated list of jigs that this trigger is compatible with.


.logger
-------

Loggers keep track of test events.  They may write test events to a file, save them on the network, print coupons at the end of a test run, or simply display "Pass" or "Fail" lights.

The following fields are valid in the [Logger] section:
* Name: A name describing this logger.
* Description: A longer paragraph describing this logger.
* Jigs: An optional list of acceptable jigs.
* Format: Describes the format of data that the logger expects.  Can be "tsv" or "json".  Defaults to "tsv" if unspecified.
* ExecStart: Name of a program to run in order to perform logging.


.interface
----------

Interfaces are similar to Loggers and Triggers, and can perform similar roles.

The following fields can go in the [Interface] section:
* Name: A name describing this interface.
* Description: A longer paragraph describing this interface.
* ExecStart: The program to invoke to act as the interface.
* WorkingDirectory: Directory to run the ExecStart program from.
* Format: Describes the interface format.  May be "text" or "json".  Defaults to "text" if unspecified.
* Jigs: A list of jigs that this interface is compatible with.  Omit this field for "all".

.coupon
-------


.updater
--------

An Updater configuration can be used to read update files off of USB drives or off of the network.