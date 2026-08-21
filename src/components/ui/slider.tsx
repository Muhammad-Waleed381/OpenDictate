import type * as React from "react";
import { cn } from "@/lib/utils";

interface SliderProps
  extends Omit<
    React.InputHTMLAttributes<HTMLInputElement>,
    "type" | "onChange"
  > {
  value: number;
  min?: number;
  max?: number;
  onChange: (value: number) => void;
}

/** Themed range input: track shows accent fill up to the thumb. */
function Slider({
  value,
  min = 0,
  max = 100,
  onChange,
  className,
  style,
  ...props
}: SliderProps) {
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <input
      type="range"
      min={min}
      max={max}
      value={value}
      onChange={(e) => onChange(Number(e.target.value))}
      className={cn(
        "h-2 w-full cursor-pointer appearance-none rounded-full border-2 border-border bg-secondary outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-slider-thumb]:size-4 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-border [&::-webkit-slider-thumb]:bg-brand",
        className,
      )}
      style={{
        background: `linear-gradient(to right, var(--brand) ${pct}%, var(--secondary) ${pct}%)`,
        ...style,
      }}
      {...props}
    />
  );
}

export { Slider };
