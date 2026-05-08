#!/usr/bin/perl
# Buffer → StringBuilder migration for text-only files.
# Простой substitute (CRLF-safe, через :raw):
#   Buffer.new()              → StringBuilder.new()
#   Buffer.with_capacity(N)   → StringBuilder.with_capacity(N)
#   Buffer.from(s)            → StringBuilder.from(s)
#   .add_str(s)               → .append(s)
#   .add_char(c)              → .append(c)
#   .into_str_unchecked()     → .into()
#
# НЕ трогаем .add_byte и .add_bytes — они требуют семантического анализа.

use strict;
use warnings;

my @files = qw(
    std/crypto/bcrypt.nv
    std/encoding/base64.nv
    std/encoding/csv.nv
    std/encoding/hex.nv
    std/encoding/ini.nv
    std/encoding/toml.nv
    std/identifiers/ulid.nv
    std/identifiers/uuid.nv
    std/path/path.nv
    std/text/diff.nv
    std/text/markdown_minimal.nv
    std/text/regex.nv
    std/time/duration.nv
);

my $total = 0;

for my $path (@files) {
    open my $fh, '<:raw', $path or do { warn "$path: $!"; next };
    my $content = do { local $/; <$fh> };
    close $fh;

    my $orig = $content;

    # Refuse if file has add_byte or add_bytes — semantic risk
    if ($content =~ /\.add_byte[s]?\(/) {
        print "$path: SKIP — has .add_byte/add_bytes (semantic risk)\n";
        next;
    }

    # Constructors
    $content =~ s/\bBuffer\.new\(\)/StringBuilder.new()/g;
    $content =~ s/\bBuffer\.with_capacity\(/StringBuilder.with_capacity(/g;
    $content =~ s/\bBuffer\.from\(/StringBuilder.from(/g;

    # Methods
    $content =~ s/\.add_str\(/.append(/g;
    $content =~ s/\.add_char\(/.append(/g;
    $content =~ s/\.into_str_unchecked\(\)/.into()/g;

    if ($content ne $orig) {
        open my $out, '>:raw', $path or die;
        print $out $content;
        close $out;
        my $count = ($orig =~ /\bBuffer\.|\.add_str\(|\.add_char\(|\.into_str_unchecked\(\)/g);
        print "$path: migrated\n";
        $total++;
    }
}

print "\nTotal files: $total\n";
