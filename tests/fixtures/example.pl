#!/usr/bin/env perl
use strict;
use warnings;

package Example;

sub process_data {
    my ($value) = @_;
    return $value + 1;
}

sub helper {
    return "helper";
}

if (@ARGV && $ARGV[0] eq 'run') {
    print process_data(3), "\n";
}

1;
