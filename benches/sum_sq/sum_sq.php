<?php
// Same loop in PHP 8.4. Compared against the PHC + C versions.
$sum = 0;
for ($i = 0; $i < 100000000; $i++) $sum += $i * $i;
echo "sum: $sum\n";
