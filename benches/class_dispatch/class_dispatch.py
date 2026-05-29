import sys


# Count from stdin; CPython can't inline the method so every iteration
# is a real dispatch. Result printed so the loop is observed.
class Counter:
    def __init__(self, start):
        self.value = start

    def inc(self):
        self.value += 1

    def get(self):
        return self.value


count = int(sys.stdin.readline().strip() or 0)
c = Counter(0)
for _ in range(count):
    c.inc()
print(f"counter = {c.get()}")
