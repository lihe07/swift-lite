import random
import numpy as np


def enc(origin):
    if isinstance(origin, list):
        print("[")
        for i in origin:
            enc(i)
            print(",")
        print("]")
        return
    elif isinstance(origin, tuple):
        print("(")
        for i in origin:
            enc(i)
            print(",")
        print(")")
        return

    def random_base_repr(num):
        base = random.choice([2, 8, 10, 16])
        prefix = {2: "0b", 8: "0o", 10: "", 16: "0x"}
        return f"{prefix[base]}{np.base_repr(num, base)}"

    num = origin

    operations = ["+", "-", "*", "<<"]

    def rev_op(op):
        if op == "+":
            return "-"
        elif op == "-":
            return "+"
        elif op == "*" and isinstance(origin, int):
            return "//"
        elif op == "*" and isinstance(origin, float):
            return "/"
        elif op == "/":
            return "*"
        elif op == "<<":
            return ">>"
        elif op == ">>":
            return "<<"

    applied = []
    operands = []

    depth = random.randint(10, 20)

    # apply operations, randomly
    # generate operands

    for _ in range(depth):
        op = random.choice(operations)
        applied.append(op)
        if op in ["+", "-", "*", "/"]:
            operand = random.randint(1, 100)
        else:
            operand = random.randint(1, 100)
        operands.append(operand)

        num = eval(f"{num} {op} {operand}")

    # print the result to recover the original number

    c = "(" * depth
    c += random_base_repr(num)

    for op, operand in zip(applied[::-1], operands[::-1]):
        op = rev_op(op)
        operand = random_base_repr(operand)
        c += f"{op}{operand})"

    print(c)
    return c


origin = eval(input(""))
enc(origin)
