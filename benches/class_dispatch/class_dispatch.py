class Counter:
    def __init__(self, start):
        self.value = start
    def inc(self):
        self.value += 1
    def get(self):
        return self.value

c = Counter(0)
for _ in range(10000000):
    c.inc()
print("done")
