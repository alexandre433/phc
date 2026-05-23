# Same loop in CPython 3.11. Compared against the PHC + C versions.
total = 0
for i in range(100000000):
    total += i * i
print(f"sum: {total}")
