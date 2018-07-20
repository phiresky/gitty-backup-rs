


## Why not just use Git

There are some things that sadly make git itself unsuitable for this task. Interestingly, many of these problems can probably be solved within git itself without breaking backwards compatibility.

1. Hardcoded zlib compression

    Every object (esp. file) in git is immediately compressed with zlib. This can not be turned off. The package format etc. would need some changes to be able to use other or no compression.
2. Inflexible delta compression

    Delta compression can be turned off per file using gitattributes. But the way delta compression works is not flexible: For every pack file, the objects are ordered heuristically (using time, basename, etc) and then each object is delta compressed using a fixed number of surrounding objects.


3. Removing / reducing historical data is not possible

    Git it enforces the existence of 100% of all data since that time point. There were proposed patches to allow cloning of the blobs only from a subdirectory, but as is git will immediately throw errors if any blobs are missing.

    This program allows removing intermediate versions of files, without having to change history. You can have hourly snapshots for a week and then only daily snapshots for older data. Only metadata changes (esp. file names) have to be retained.

    The only option for reducing history size in git is a fixed historical cut off (called shallow repository).