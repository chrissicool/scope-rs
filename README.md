# scope

scope-rs collects source code files starting from the given directory.
It creates a file index and creates ctags(1) and cscope(1) databases from it.
Files are inspected based on extensions and mimetypes, in that order.
Multiple drivers are available.
By default, the best one is chosen automatically.
File inspection is performed in parallel.
scope-rs skips commonly known version control files and directories
as well as backup files.

scope-rs needs cscope(1) and Exuberant ctags(1) in PATH.

## Why

cscope(1) and ctags(1) need to know which files to include
in their tags databases.
That is cumbersome for files that have no file extension,
e.g. scripts or C++ header files.
scope-rs solves this by looking at a file's mime type to decide
wheter to feed it to the tags database.

## Usage

Consult the integrated help:

```
$ scope -h
```

Scope-rs is designed to be usable by default without any parameters.
Create ctags and cscope databases in the current directory:

```
$ scope
```

The databases use relative paths.
You can move the sub-tree with the databases around in your filesystem.

## TODO

The exclude handling is clumsy at best.
There needs to be a better way to handle this.

## History

There was a rather sophisticated Perl script that did the job pretty well.
Porting this script to Rust helped me learn the language on a real-world
project that I am using regularly.
