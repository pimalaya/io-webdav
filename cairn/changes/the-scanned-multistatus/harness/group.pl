# group.pl <label>: reads callgrind_annotate --auto=no output on stdin and
# prints the instruction share of each function group.
my %g; my $t;
while (<STDIN>) {
  next unless /^\s*([\d,]+) \(\s*[\d.]+%\)\s+(.*)$/;
  my ($n, $f) = ($1, $2); $n =~ s/,//g; $f =~ s/^[^:]*://; $f =~ s/ \[.*//;
  if ($f =~ /PROGRAM TOTALS/) { $t = $n; next }
  my $c = $f =~ /malloc|free|realloc|_int_|consolidate|calloc/ ? "alloc"
    : $f =~ /resolve_reference/ ? "decode-refs"
    : $f =~ /decode_text|push_lines|decode_attr|memchr2/ ? "decode"
    : $f =~ /XmlScanner|XmlAttrs|memchr3|memmem|memchr::memchr_raw/ ? "scan"
    : $f =~ /memcmp|bcmp/ ? "memcmp"
    : $f =~ /memcpy|memmove/ ? "memcpy"
    : $f =~ /from_utf8/ ? "utf8"
    : $f =~ /typed::|NodeDe|Children|Group|Siblings/ ? "typed"
    : $f =~ /Deserialize|Visitor|serde/ ? "serde-derive"
    : $f =~ /consumer|wdxml/ ? "map"
    : "other";
  $g{$c} += $n;
}
printf "%-18s %6.1fM Ir:", $ARGV[0], $t / 1e6;
printf " %s %.0f%%", $_, 100 * $g{$_} / $t for sort { $g{$b} <=> $g{$a} } keys %g;
print "\n";
