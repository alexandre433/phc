<?php
class Counter {
    public int $value;
    public function __construct(int $start) { $this->value = $start; }
    public function inc(): void { $this->value++; }
    public function get(): int { return $this->value; }
}

$c = new Counter(0);
for ($i = 0; $i < 10000000; $i++) {
    $c->inc();
}
echo "done\n";
