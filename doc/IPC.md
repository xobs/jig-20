Inter-process Communication
===========================

The testing framework launches sub-processes and communicates with them through stdin and/or stdout.  Some initial variables are passed as environment variables, but most work takes place at runtime.

CFTI is based on the idea of CGI, where any program can be connected to the server very easily.  CFTI breaks from CGI in that there are certain classes of long-running processes, whereas CGI tends to be a one-off interaction.

All records are line-ordered, with one record per line.  They may have unlimited length, though in practice be reasonable.

Logger - TSV
------------

Logger units that accept TSV will receive a stream of tab-separated files.  Records will arrive with the following fields:

    <message-type>   <unit>    <unit-type>    <unix-time-secs>    <unix-time-nsecs>    <message>

The &lt;message> field will replace returns with "\n", tabs with "\t", and backslashes with "\\".  Other values will be passed through unaltered.

Logger - JSON
-------------

Logger units that accept JSON will receive a stream of line-delimited JSON records.  At a minimum, the same records as TSV will appear.  An example record is:

    {"message_type":2,"unit":"<internal>","unit_type":"<internal>","unix_time":1485942257,"unix_time_nsecs":149052500,"message":"I loop: 0"}

Interface - Text
---------------

A simple interface may request a text protocol, in which case it is similar to most other line-oriented protocols such as HTTP or SMTP.  Verbs are a single word, followed by a space, followed by one or more arguments.  If there are no arguments, then the space may be omitted.

Verbs are case-insensitive, however they are presented here in all caps due to tradition.

Verbs sent by the CFTI server:

 * HELLO version - The first thing sent by the infrastructure.  Identifies itself as a CFTI interface.
 * JIG jigname - Sent at startup, and if/when the jig is changed.
 * SCENARIOS [list] - Sent whenever the list of scenarios is updated.  [list] is a whitespace-separated list of available scenarios.
 * SCENARIO [item] - Sent whenever a scenario is chosen.  This will happen automatically at startup.
 * TESTS [list] - Sent whenever the list of tests is updated, or whenever a new scenario is chosen.
 * START [scenario] - Sent at the start, when a scenario is begun.
 * RUNNING [test] - Indicates the current test is being run.
 * DESCRIBE [type] [field] [item] [value] - Describes a [type] (scenario, jig, or test) field of [field] (name or description) of item [item] to be [value].  E.g. "DESCRIBE TEST NAME simpletest A simple test".
 * PASS [test] - Indicates a particular item passed.
 * FAIL [test] - Indicates a particular item failed.
 * SKIP [test] - Indicates a test was skipped, likely due to an earlier failure.
 * FINISH [result] [scenario] - Sent after all tests have been run or skipped, or if the test has aborted.  Result is an HTTP error code, with "200" indicating success.
 * LOG [log-item] - Relays logging data via the Interface connection.  See Logger - TSV for the log-item format.
 * PING [id] - Sent occasionally to make sure the program is still alive.  Must echo [id] back.
 * EXIT - Shuts down the server.

Verbs that may be sent by the CFTI client:

 * HELLO identifier - Identify this particular client.  Optional.
 * JIG - Request the current jig name.
 * SCENARIOS - Request the list of scenarios.
 * SCENARIO [selection] - Select a particular scenario.
 * TESTS - Request a list of tests.
 * START - Begins running the current scenario.
 * ABORT - Stop the current scenario without running all tests.
 * PONG [id] - Respond to a PING command, to indicate the program is still active.  Must respond withing five seconds.
 * LOG [message] - Log a message to the log bus.  Note that it will be echoed back, so be careful not to create an infinite loop.