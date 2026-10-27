import { describe, it, expect } from "vitest";
import {
  spanFor,
  innerRadius,
  midAngle,
  pointAt,
  angleOf,
  indexAt,
  hitTest,
  sectorPath,
  labelAnchor,
  outwardUnit,
  withInsert,
  withMove,
  withRemove,
  RING_GEOM,
} from "./ringGeometry";

const CX = 200;
const CY = 200;
const R = RING_GEOM.R; // 150

describe("spanFor", () => {
  it("n<=0 返回整圆", () => {
    expect(spanFor(0)).toBeCloseTo(Math.PI * 2);
    expect(spanFor(-3)).toBeCloseTo(Math.PI * 2);
  });

  it("n>0 均分圆周", () => {
    expect(spanFor(4)).toBeCloseTo(Math.PI / 2);
    expect(spanFor(2)).toBeCloseTo(Math.PI);
  });
});

describe("innerRadius", () => {
  it("按 holeRatio 计算", () => {
    expect(innerRadius(150)).toBeCloseTo(150 * RING_GEOM.holeRatio);
  });

  it("结果落在合法范围 [0.2, 0.75]", () => {
    expect(innerRadius(150)).toBeGreaterThanOrEqual(150 * 0.2);
    expect(innerRadius(150)).toBeLessThanOrEqual(150 * 0.75);
  });
});

describe("midAngle", () => {
  it("第 i 个扇区的中心角 = i × span", () => {
    expect(midAngle(0, 4)).toBeCloseTo(0);
    expect(midAngle(1, 4)).toBeCloseTo(Math.PI / 2);
    expect(midAngle(3, 4)).toBeCloseTo((3 * Math.PI) / 2);
  });
});

describe("pointAt / angleOf 互逆", () => {
  it("12 点方向 angle=0", () => {
    const p = pointAt(CX, CY, R, 0);
    expect(p.x).toBeCloseTo(CX);
    expect(p.y).toBeCloseTo(CY - R);
  });

  it("3 点方向 angle=π/2（y 轴向下，顺时针）", () => {
    const p = pointAt(CX, CY, R, Math.PI / 2);
    expect(p.x).toBeCloseTo(CX + R);
    expect(p.y).toBeCloseTo(CY);
  });

  it("angleOf 是 pointAt 的逆", () => {
    for (const a of [0, Math.PI / 4, Math.PI / 2, Math.PI, (3 * Math.PI) / 2]) {
      const p = pointAt(CX, CY, R, a);
      expect(angleOf(CX, CY, p.x, p.y)).toBeCloseTo(a);
    }
  });
});

describe("indexAt", () => {
  it("四个象限各落到对应扇区", () => {
    expect(indexAt(0, 4)).toBe(0); // 12 点
    expect(indexAt(Math.PI / 2, 4)).toBe(1); // 3 点
    expect(indexAt(Math.PI, 4)).toBe(2); // 6 点
    expect(indexAt((3 * Math.PI) / 2, 4)).toBe(3); // 9 点
  });

  it("接近 2π 的角度回到扇区 0", () => {
    expect(indexAt(Math.PI * 1.99, 4)).toBe(0);
  });
});

describe("hitTest", () => {
  it("中心孔命中 center", () => {
    expect(hitTest(CX, CY, CX, CY, 4)).toEqual({ kind: "center" });
  });

  it("环外命中 outside", () => {
    expect(hitTest(CX, CY, CX + 500, CY, 4)).toEqual({ kind: "outside" });
  });

  it("12 点环内命中扇区 0", () => {
    expect(hitTest(CX, CY, CX, CY - 100, 4)).toEqual({ kind: "segment", index: 0 });
  });

  it("3 点环内命中扇区 1", () => {
    expect(hitTest(CX, CY, CX + 100, CY, 4)).toEqual({ kind: "segment", index: 1 });
  });

  it("n=0 时环内命中 empty", () => {
    expect(hitTest(CX, CY, CX, CY - 100, 0)).toEqual({ kind: "empty" });
  });
});

describe("sectorPath", () => {
  it("n===1 画整圆：外圈 + 内圈两条闭合子路径", () => {
    const path = sectorPath(0, 0, 100, 50, 0, 1);
    expect(path.match(/Z/g)?.length).toBe(2);
    expect(path.startsWith("M")).toBe(true);
  });

  it("n>1 画单扇区：M + 外弧 + L + 内弧 + Z", () => {
    const path = sectorPath(0, 0, 100, 50, 0, 4);
    expect(path.startsWith("M")).toBe(true);
    expect(path).toContain("A");
    expect(path).toContain("L");
    expect(path.trimEnd().endsWith("Z")).toBe(true);
  });
});

describe("labelAnchor / outwardUnit", () => {
  it("标签锚点位于内外半径中线", () => {
    const anchor = labelAnchor(0, 0, 100, 50, 0, 4);
    expect(anchor.x).toBeCloseTo(0);
    expect(anchor.y).toBeCloseTo(-75); // -(100+50)/2，12 点方向
  });

  it("扇区 0 向外单位向量向上", () => {
    const v = outwardUnit(0, 4);
    expect(v.x).toBeCloseTo(0);
    expect(v.y).toBeCloseTo(-1);
  });

  it("扇区 1 向外单位向量向右", () => {
    const v = outwardUnit(1, 4);
    expect(v.x).toBeCloseTo(1);
    expect(v.y).toBeCloseTo(0);
  });
});

describe("顺序运算（设置页圆环编辑器）", () => {
  it("withInsert 在指定槽位插入", () => {
    expect(withInsert(["a", "b"], "x", 1)).toEqual(["a", "x", "b"]);
    expect(withInsert(["a", "b"], "x", 0)).toEqual(["x", "a", "b"]);
    expect(withInsert(["a", "b"], "x", 2)).toEqual(["a", "b", "x"]);
  });

  it("withInsert 越界槽位被 clamp", () => {
    expect(withInsert(["a", "b"], "x", 99)).toEqual(["a", "b", "x"]);
    expect(withInsert(["a", "b"], "x", -5)).toEqual(["x", "a", "b"]);
  });

  it("withMove 移动到目标位置", () => {
    expect(withMove(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"]);
    expect(withMove(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
  });

  it("withMove 同位置或越界源返回原数组", () => {
    expect(withMove(["a", "b", "c"], 1, 1)).toEqual(["a", "b", "c"]);
    expect(withMove(["a", "b", "c"], -1, 0)).toEqual(["a", "b", "c"]);
    expect(withMove(["a", "b", "c"], 9, 0)).toEqual(["a", "b", "c"]);
  });

  it("withRemove 移除指定项", () => {
    expect(withRemove(["a", "b", "c"], 1)).toEqual(["a", "c"]);
    expect(withRemove(["a", "b", "c"], 0)).toEqual(["b", "c"]);
  });
});
