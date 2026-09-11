import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import ArtifactPanel from "./ArtifactPanel";
import PromptCatalogSection from "./PromptCatalogSection";
import { factoryPromptSettings } from "./promptCatalog";
import { createMemoryDesktopClient } from "./desktopClient";
import type { ArtifactProjection, PromptSettings } from "./types";
import { renderWithLocale as render } from "./i18n/testUtils";

const content = {
  briefProtocol: "textbook-v2", documentKind: "textbook",
  takeaway: "建立条件概率的认识", keywords: ["概率", "条件"],
  learningScope: "当前PDF包含多个章节的节选", motivation: "用新信息更新判断",
  prerequisites: "理解事件和条件概率", knowledgeStructure: "由定义连接到更新关系",
  coreKnowledge: "在给定条件下理解概率关系", masteryGoals: "能辨认条件方向",
  connections: "联系后续推断方法",
};
const brief: ArtifactProjection = {
  id:"brief",paperId:"p",revisionId:"r",ocrRevisionId:null,kind:"brief",objectKey:"",
  version:1,status:"ready",content,overrides:{},evidence:[],dependencySnapshot:{},
  providerNodeId:null,createdAt:"2026-09-11",
};
const props = {
  artifacts:[brief],activeArtifactId:brief.id,activeBlockId:null,displayCropSrc:"",
  lensQa:[],lensQaBusy:false,onSelect:vi.fn(),onJump:vi.fn(),onAskLens:vi.fn(),
  onTransferLens:vi.fn(),onSetOverride:vi.fn(),onGenerateBrief:vi.fn(),
};
describe("textbook learning artifacts",()=>{
  it("shows all learning fields from the stored protocol without paper-only headings",()=>{
    render(<ArtifactPanel {...props} />);
    for(const title of ["学习范围与定位","为什么学习这些内容","必要基础","知识结构","核心知识与方法","应形成的能力","后续衔接与应用"]) {
      expect(screen.getByRole("heading",{name:title})).toBeInTheDocument();
    }
    expect(screen.getByText(content.masteryGoals)).toBeInTheDocument();
    expect(screen.queryByText(/论文分类与主题定位|审辨式评估|未解问题与后续方向/)).not.toBeInTheDocument();
  });
  it("keeps keyword editing available without losing learning content",async()=>{
    const onUpdateTags=vi.fn().mockResolvedValue(undefined);
    render(<ArtifactPanel {...props} onUpdateTags={onUpdateTags} />);
    await userEvent.setup().click(screen.getByRole("button",{name:"移除标签 概率"}));
    expect(onUpdateTags).toHaveBeenCalledWith("p",["条件"]);
    expect(screen.getByText(content.coreKnowledge)).toBeInTheDocument();
  });
  it("reads old textbook fields with teaching labels and leaves paper labels intact",()=>{
    const old:ArtifactProjection={...brief,content:{takeaway:"旧版概要",keywords:[],classification:"教材属性",context:"基础",coreMethod:"原有讲解",findings:"原有结论",evaluation:"原有学习建议",futureWork:"后续联系"}};
    const {rerender}=render(<ArtifactPanel {...props} artifacts={[old]} documentKind="textbook" />);
    expect(screen.getByRole("heading",{name:"核心概念与讲解思路"})).toBeInTheDocument();
    expect(screen.getByText("原有讲解")).toBeInTheDocument();
    rerender(<ArtifactPanel {...props} artifacts={[old]} documentKind="paper" />);
    expect(screen.getByText("🛠️ 核心方法与设计")).toBeInTheDocument();
  });
  it("describes current material in the textbook empty state",()=>{
    render(<ArtifactPanel {...props} artifacts={[]} activeArtifactId="" preferBrief documentKind="textbook" />);
    expect(screen.getByText(/当前材料的学习范围/)).toBeInTheDocument();
    expect(screen.queryByText(/论文的问题|这一章学什么/)).not.toBeInTheDocument();
  });
  it("shows the actual textbook Brief protocol in prompt settings",async()=>{
    const settings=factoryPromptSettings();
    const user=userEvent.setup();
    render(<PromptCatalogSection settings={settings} busy={false} onSave={vi.fn()} onRestorePrevious={vi.fn()} onRestoreDefault={vi.fn()}/>);
    await user.click(screen.getByRole("tab",{name:"教材"}));
    await user.click(screen.getByRole("button",{name:"Brief"}));
    expect(screen.getByLabelText("教材输出结构")).toHaveTextContent("教材九字段结构");
  });
  it("retains textbook protocol and previous content through repeated default restores in memory",async()=>{
    const client=createMemoryDesktopClient();
    const before=await client.open<PromptSettings>("get_prompt_settings");
    await client.command("save_prompt_slot",{request:{slot:"orientation_pack",kind:"textbook",text:"自定义教材稿"}});
    await client.command("restore_prompt_default",{request:{slot:"orientation_pack",kind:"textbook"}});
    await client.command("restore_prompt_default",{request:{slot:"orientation_pack",kind:"textbook"}});
    const restored=await client.command<PromptSettings>("restore_prompt_previous",{request:{slot:"orientation_pack",kind:"textbook"}});
    expect(restored.slots.orientation_pack.textbook).toMatchObject({text:"自定义教材稿",outputProtocol:"textbook-v2"});
    expect(restored.slots.orientation_pack.paper).toEqual(before.slots.orientation_pack.paper);
  });
});
