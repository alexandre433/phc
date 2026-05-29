<?php
// Count from stdin; PHP can't inline the method so every iteration is
// a real dispatch. Result printed so the loop is observed.
$count = (int)trim(fgets(STDIN));

class Counter {
    public int $value;
    public function __construct(int $start) { $this->value = $start; }
    public function inc(): void { $this->value++; }
    public function get(): int { return $this->value; }
}

$c = new Counter(0);
for ($i = 0; $i < $count; $i++) {
    $c->inc();
}
echo "counter = " . $c->get() . "\n";
