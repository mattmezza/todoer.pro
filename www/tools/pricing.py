"""Recalculate local price points from the versioned root pricing catalog."""
import json
from decimal import Decimal, ROUND_CEILING
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def calculate(catalog):
    rates = catalog['conversion']['units_per_eur']
    base_rate = Decimal(str(rates[catalog['base_currency']]))
    multiplier = 1 + Decimal(str(catalog['conversion']['fee_percent'])) / 100
    for plan in catalog['plans']:
        prices = {}
        for currency in catalog['currencies']:
            values = {}
            for source, dest in [('base_amount_minor', 'amount_minor'), ('regular_amount_minor', 'regular_amount_minor')]:
                target = Decimal(plan[source]) / 100 * Decimal(str(rates[currency])) / base_rate
                if currency == catalog['base_currency']:
                    amount = target
                else:
                    target *= multiplier
                    if plan['interval'] == 'year':
                        amount = target.to_integral_value(rounding=ROUND_CEILING)
                    else:
                        amount = ((target - 9) / 10).to_integral_value(rounding=ROUND_CEILING) * 10 + 9
                values[dest] = int(amount * 100)
            prices[currency] = values
        plan['prices'] = prices
    return catalog


if __name__ == '__main__':
    path = ROOT / 'pricing.json'
    path.write_text(json.dumps(calculate(json.loads(path.read_text())), indent=2) + '\n')
