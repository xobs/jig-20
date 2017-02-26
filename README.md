
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

    <message-type>\t<unit>\t<unit-type>\t<unix-time>\t<unix-time-nsecs>\t<message>

* message-type: A string indication of the type of message.  Identification strings are [a-z0-9\-].
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


Implementation Progress
=======================

  * Scenarios
    * Dependency resolution
    * Dependency ordering
    * Test start/stop
    * Scenario duration
    * _Hung ExecStart/ExecStop_
  * Tests
    * Simple tests
    * Test logging
    * Test timeout
    * Daemon tests
    * Hung tests
    * _ExecStop_
    * _Extra Pipes_
    * _Provides_
  * Interfaces
    * Basic interface interaction
    * _Ping/Pong keepalive_
  * Jigs
    * TestFile
    * TestProgram
  * _Triggers_
  * _Updaters_
  * _Services_
  * _Coupons_
  * _Localization_
  * _Live-reload of files_