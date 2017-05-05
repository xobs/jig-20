The following message types are defined.  These may be present as part of any sort of received message.

Other message types are possible; this list is non-exhaustive.

+-------------------+------------------------------------------------------+
| Name              | Description                                          |
+===================+======================================================+
| stdout            | The accompanying string was read from the unit's     |
|                   | stdout stream                                        |
+-------------------+------------------------------------------------------+
| stderr            | The accompanying string was read from the unit's     |
|                   | stderr stream                                        |
+-------------------+------------------------------------------------------+
| config-error      | The named unit file was misconfigured (i.e. it       |
|                   | contains a syntax error, failed dependency, or other |
|                   | reason for not loading.)                             |
+-------------------+------------------------------------------------------+
| unloading         | The specified unit file will be removed.             |
+-------------------+------------------------------------------------------+
| debug             | Internal debug message.                              |
+-------------------+------------------------------------------------------+
| 
+-------------------+------------------------------------------------------+
