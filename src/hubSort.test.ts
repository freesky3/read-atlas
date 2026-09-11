import { describe, it, expect } from "vitest";
import { parseChapterNumber, compareChapterNumbers, chapterSortKey, mergeManualOrder, hubSortAvailability, moveItem, applyHubSort } from "./hubSort";
import type { DocumentCard } from "./types";

function doc(id: string, collection: string, chapter?: string, importedAt = "2026-08-17T10:00:00Z"): DocumentCard {
  return {
    id,
    revisionId: `rev-${id}`,
    title: id,
    authors: "",
    year: "",
    pages: 1,
    collection,
    kind: "textbook",
    sha256: id,
    pdfPath: `${collection}/${id}.pdf`,
    sourceStatus: "ready",
    briefStatus: "ready",
    chapterNumber: chapter,
    importedAt,
    fileName: `${id}.pdf`,
  } as DocumentCard;
}

describe("chapterSortKey", () => {
  it("orders 1.2 before 1.10 and unknown last", () => {
    expect(chapterSortKey("1.2") < chapterSortKey("1.10")).toBe(true);
    expect(chapterSortKey("1") < chapterSortKey("附录")).toBe(true);
  });
});

describe("parseChapterNumber", () => {
  it("parses integer segments", () => {
    expect(parseChapterNumber("3")).toEqual([3]);
    expect(parseChapterNumber("3.2")).toEqual([3,2]);
    expect(parseChapterNumber("3.10")).toEqual([3,10]);
    expect(parseChapterNumber("3-1")).toEqual([3,1]);
    expect(parseChapterNumber("03")).toEqual([3]);
  });
  it("rejects garbage", () => {
    expect(parseChapterNumber("3.2 习题")).toBeNull();
    expect(parseChapterNumber("附录A")).toBeNull();
    expect(parseChapterNumber("")).toBeNull();
    expect(parseChapterNumber(undefined)).toBeNull();
  });
});

describe("compareChapterNumbers", () => {
  it("orders numeric segments", () => {
    expect(compareChapterNumbers("3", "3.2")).toBeLessThan(0);
    expect(compareChapterNumbers("3.2", "3.10")).toBeLessThan(0);
    expect(compareChapterNumbers("10", "2")).toBeGreaterThan(0);
    expect(compareChapterNumbers("3.10", "3.1")).toBeGreaterThan(0);
  });
  it("places missing after has", () => {
    expect(compareChapterNumbers(undefined, "1")).toBeGreaterThan(0);
    expect(compareChapterNumbers("1", undefined)).toBeLessThan(0);
  });
});

describe("mergeManualOrder", () => {
  it("appends new comers asc", () => {
    const a = doc("a","Book","1","2026-08-17T10:00:00Z");
    const b = doc("b","Book","2","2026-08-17T11:00:00Z");
    const c = doc("c","Book",undefined,"2026-08-17T12:00:00Z");
    expect(mergeManualOrder([a,b,c], ["a","b"]).map(d=>d.id)).toEqual(["a","b","c"]);
  });
  it("without order falls back to recent", () => {
    const a = doc("a","Book",undefined,"2026-08-17T10:00:00Z");
    const b = doc("b","Book",undefined,"2026-08-17T12:00:00Z");
    const res = mergeManualOrder([a,b], []);
    expect(res[0].id).toBe("b");
  });
});

describe("availability", () => {
  it("disables manual for all and parent with descendant", () => {
    const docs = [doc("a","Textbooks/Book","1"), doc("b","Textbooks/Book/Chapter","1")];
    expect(hubSortAvailability(docs,"")).toEqual({manual:false, chapter:false});
    expect(hubSortAvailability(docs,"Textbooks/Book")).toEqual({manual:false, chapter:false});
  });
  it("enables manual for leaf, chapter only if has number", () => {
    const docs = [doc("a","Textbooks/Book","3"), doc("b","Textbooks/Book","1")];
    expect(hubSortAvailability(docs,"Textbooks/Book")).toEqual({manual:true, chapter:true});
    const docs2 = [doc("a","Textbooks/Book",undefined)];
    expect(hubSortAvailability(docs2,"Textbooks/Book")).toEqual({manual:true, chapter:false});
  });
});

describe("moveItem", () => {
  it("before/after/end and no-op", () => {
    expect(moveItem(["a","b","c"],"a","c","before")).toEqual(["b","a","c"]);
    expect(moveItem(["a","b","c"],"a","b","after")).toEqual(["b","a","c"]);
    expect(moveItem(["a","b","c"],"c",null,"end")).toEqual(["a","b","c"]);
    expect(moveItem(["a","b","c"],"a",null,"end")).toEqual(["b","c","a"]);
    expect(moveItem(["a","b","c"],"b","b","before")).toEqual(["a","b","c"]);
  });
});

describe("applyHubSort chapter", () => {
  it("sorts chapter numerically", () => {
    const docs = [doc("a","Book","10"), doc("b","Book","2"), doc("c","Book","3.10"), doc("d","Book",undefined)];
    const res = applyHubSort(docs,"chapter",[]);
    expect(res.map(d=>d.id)).toEqual(["b","c","a","d"]);
  });
});
