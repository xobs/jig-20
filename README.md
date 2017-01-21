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


Daemons
-------

Some tests can run as daemons.  This is particularly useful for services such as OpenOCD, which must be set up in order to program some devices.


.jig
----

Jigs are physical devices that perform tests.  You will have a jig in the factory, and you should have a jig in your workshop.  Your work machine can also act as a "Jig", though it might not provide all of the same features.

The following fields are allowed in the [Jig] section:
* Name: The name of the jig and the device or product that it tests.
* Description: A longer description of the jig and the product or device being tested, up to one paragraph long.


.scenario
---------
* Tests: A space- or comma-separated list of tests to be run.  Note that you only need to specify the final test to run, as the dependency graph will fill in the rest.  If you specify multiple tests, then they will be run in the order you specify, possibly with dependency tests added in between.
* Success: A command to run if a test scenario completes successfully.
* Failure: A command to be run if a test scenario fails.


.trigger
--------

A trigger is used to start a test.  Triggers are non-repeating and events are consumed.  That is, you can send as many "start" commands as you like, but if the test is already running then they will be ignored.

The following fields are valid in the [Trigger] section:
* Name: A short string describing this trigger
* Description: A longer decsription of this trigger, up to one paragraph long.
* ExecStart: Name of the program to run to get trigger information from.
* Jig: A comma-separated list of jigs that this trigger is compatible with.


.logger
-------

Loggers keep track of test events.  They may write test events to a file, save them on the network, print coupons at the end of a test run, or simply display "Pass" or "Fail" lights.

The following fields are valid in the [Logger] section:
* Name: A name describing this logger.
* Description: A longer paragraph describing this logger.
* ExecStart: Name of a program to run in order to perform logging.


.interface
----------

Interfaces are similar to Loggers and Triggers, and can perform similar roles.


.updater
--------

An Updater configuration can be used to read update files off of USB drives or off of the network.