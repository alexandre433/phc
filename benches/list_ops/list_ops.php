<?php
$xs = [];
for ($i = 0; $i < 100000; $i++) {
    $xs[] = 100000 - $i;
}
sort($xs);
$sum = array_sum($xs);
echo "done\n";
