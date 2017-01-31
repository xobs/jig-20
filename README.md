
Jig-20 Objects
--------------

Jig-20 supports the following objects:

* **Test** - Fundamental unit of work.
* **Jig** - The device that hosts the tests, what UI it has, and what scenario to run.
* **Scenario** - A group of tests to accomplish a goal, e.g. "Factory Test", "Wifi sub-test", or "Regression Test".
* **Coupon** - A non-forgeable certificate that gets generated at the end of a complete, successful run of tests.
* **Trigger** - Source that allows for starting a test or scenario.
* **Logger** - Takes test, UI, update, and trigger data and saves it somehow.
* **Interface** - Shows testing progress.
* **Updater** - Allows for updating either the test infrastructure or the tests to run.
* **Service** - A background service that must always run.

Configuration
-------------

Configuration is accomplished through the use of ini-style configuration files stored in the configuration directory.  These are configured much in the same way as systemd.  Each configuration file is called a "unit" file.

The suffix of the unit file determines what sort of file it configures.

The name of the file, minus the suffix, is the name that will be used when referring to a particular unit from another unit, for example in Requires or Suggests.

Localization
------------

Unit files may include localization.  If a localized string is available, then it will be used.  Otherwise, it will fall back to the non-localized string.

For example:

    Name=Log Registers
    Name[zh]=登录寄存器


Communication
=============

Because CFTI is based on the idea of CGI, stdin and stdout are the primary communications channels between units and the controller.  Environment variables also serve as a one-way communications mechanism used to set up various parts of a program.

Streams begin as line-based and buffered, and normally allow switching modes by sending a special character as the first byte.

Test (Simple)
------------

Test (simple) is the most basic sort of test.  Each line printed by the unit to stdout is timestamped and logged as a test message.  Each line printed by the unit to stderr is timestamped and logged as an internal debug mesage.  The final line printed is taken to be the test result, and the return code indicates pass or fail.

Test (simple) is selected by setting "Type" to "simple" in the unit file.

Test (Daemon)
-------------

Test (daemon) are tests can run as daemons, usually to support other tests.  This is particularly useful for services such as OpenOCD, which must be set up in order to program some devices.

Daemons are expected to call fork() within the "Timeout" period.

The DaemonCheck= parameter describes a simple program that can be used to determine if the daemon is alive and ready.  If present, when the daemon is first started up, this program will be called repeatedly until it returns success.  Thereafter, it will be called occasionally to determine if the daemon is still alive.

If the daemon exits or the DaemonCheck= fails after startup, then this unit is marked as Failed.

Daemons can communicate with the master program using stdout and stderr, just like Simple tests.


Trigger
-------

A Trigger unit describes a means of starting (or sometimes stopping) a test.  Simple triggers might be a button, a keypress, or a network connection.

Triggers function by sending strings to stdout.  The following strings are recognized:

    * Ready -- This must be the first string printed, and indicates the trigger has started up.
    * Monitor -- Causes the test infrastructure to print test status.  Can be used by e.g. "testing status" lights.
    * Go -- Starts the test.  If a test is already running, this command is ignored.
    * Stop -- Stops the currently-running test.
    * Pass -- Printed by the infrastructure, indicates the test completed successfully.
    * Fail -- Printed by the infrastructure, indicates the test failed.


Logger
------

A logger takes messages that have been printed by the various commponents and saves them to a file.

The default logger expects tab-separated data, which will have the following format:

    <message-type><unit><unit-type><unix-time><unix-time-nsecs><message>

* message-type: A numerical indication of the type of message.  0 is internal messages such as test-start, 1 is test log output from various units, 2 is internal debug log.
* unit: The name of the unit that generated the message.
* unit-type: The type of unit, such as "test", "logger", "trigger", etc.
* unix-time: Number of seconds since the epoch
* unix-time-nsecs: Number of nanoseconds since the epoch
* message: Textual representation of the message, minus linefeeds.


Jig
---

A Jig is a single-purpose piece of hardware.  As such, many tests are bound to a particular jig.

Jigs do not have a CFTI protocol because they are merely objects, and don't have any state.  They simply exist.

The Jig unit file does have a TestFile and a TestProgram parameter that can be used to determine if the interface is running on a particular jig.  If this command exits 0, then that indicates that we are running on a particular jig.  For example, this may test for the existance of a file, twiddle some pins and check for a value, ping something on I2C, or monitor the network.



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
* Type: One of "simple" or "daemon".  For "simple" tests, the return code will indicate pass or fail, and each line printed will be considered progress.  For "daemon", the process will be forked and left to run in the background.  See "daemons" below.
* CompatibleJigs: A comma-separated list of jigs that this test is compatible with.  If unspecified, any jig is acceptable.
* ExecStart: The command to run as part of this test.
* ExecStopFail: When stopping tests, if the test failed, then this stop command will be run.
* ExecStopSuccess: When stopping tests, if the test succeeded, then this stop command will be run.
* ExecStop: When tests are completed, this command is run to clean things up.  If either ExecStopSuccess or ExecStopFail are present, then this command will be skipped.
* DaemonCheck: A command to run to test whether the daemon is ready.

.jig
----

Jigs are physical devices that perform tests.  You will have a jig in the factory, and you should have a jig in your workshop.  Your work machine can also act as a "Jig", though it might not provide all of the same features.

The following fields are allowed in the [Jig] section:
* Name: The name of the jig and the device or product that it tests.
* Description: A longer description of the jig and the product or device being tested, up to one paragraph long.
* TestProgram: Optional path to a program to determine if this is the jig we're running on.
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
* Timeout: Maximum number of seconds this scenario should take.


.trigger
--------

A trigger is used to start a test.  Triggers are non-repeating and events are consumed.  That is, you can send as many "start" commands as you like, but if the test is already running then they will be ignored.

The following fields are valid in the [Trigger] section:
* Name: A short string describing this trigger
* Description: A longer decsription of this trigger, up to one paragraph long.
* ExecStart: Name of the program to run to get trigger information from.
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
* Format: Describes the interface format.  May be "tsv" or "json".  Defaults to "tsv" if unspecified.


.coupon
-------


.updater
--------

An Updater configuration can be used to read update files off of USB drives or off of the network.